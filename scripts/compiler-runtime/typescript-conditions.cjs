'use strict';

// The frozen Svelte extension of no-unnecessary-condition uses checker types,
// except for mutable component references in reactive statements/templates.
// This module consumes that scope-aware service; it never guesses from names.
function analyze(session, options = {}) {
  const { ts, checker, sourceFile, compilerOptions } = session;
  const F = ts.TypeFlags;
  const diagnostics = [];
  const messages = {
    alwaysTruthy: 'Unnecessary conditional, value is always truthy.',
    alwaysFalsy: 'Unnecessary conditional, value is always falsy.',
    alwaysTruthyFunc: 'This callback should return a conditional, but return is always truthy.',
    alwaysFalsyFunc: 'This callback should return a conditional, but return is always falsy.',
    neverNullish: 'Unnecessary conditional, expected left-hand side of `??` operator to be possibly null or undefined.',
    alwaysNullish: 'Unnecessary conditional, left-hand side of `??` operator is always `null` or `undefined`.',
    literalBooleanExpression: 'Unnecessary conditional, both sides of the expression are literal values.',
    noOverlapBooleanExpression: 'Unnecessary conditional, the types have no overlap.',
    never: 'Unnecessary conditional, value is `never`.',
    neverOptionalChain: 'Unnecessary optional chain on a non-nullish value.',
    noStrictNullCheck: 'This rule requires the `strictNullChecks` compiler option to be turned on to function correctly.',
  };
  const strictNull = compilerOptions.strict ? compilerOptions.strictNullChecks !== false : compilerOptions.strictNullChecks;
  function report(node, id) {
    const range = session.range(node);
    if (range) diagnostics.push({ message: messages[id], ...range });
  }
  if (!strictNull && !options.allowRuleToRunWithoutStrictNullChecksIKnowWhatIAmDoing) {
    diagnostics.push({ message: messages.noStrictNullCheck, start: 0, end: 0 });
  }
  const parts = type => type.isUnion() ? type.types.flatMap(parts) : [type];
  const has = (type, flags) => parts(type).some(part => !!(part.flags & flags));
  const uncertain = type => !type || has(type, F.Any | F.Unknown | F.TypeParameter);
  const nullish = type => has(type, F.Null | F.Undefined);
  function truthyLiteral(type) {
    if (type.flags & F.BooleanLiteral) return type.intrinsicName === 'true';
    if (type.flags & (F.StringLiteral | F.NumberLiteral)) return !!type.value;
    if (type.flags & F.BigIntLiteral) return type.value.base10Value !== '0';
    return false;
  }
  const possiblyFalsy = type => parts(type).some(part => !truthyLiteral(part) && !!(part.flags & F.PossiblyFalsy));
  const possiblyTruthy = type => parts(type).some(part => !(part.flags & F.Never) && (!((part.flags & (F.Null | F.Undefined | F.Void)) || ((part.flags & F.BooleanLiteral) && part.intrinsicName === 'false') || ((part.flags & (F.StringLiteral | F.NumberLiteral)) && !part.value) || ((part.flags & F.BigIntLiteral) && part.value.base10Value === '0'))));
  const literal = type => !!(type.flags & (F.BooleanLiteral | F.Null | F.Undefined)) || type.isLiteral();
  const unwrap = node => ts.isParenthesizedExpression(node) ? unwrap(node.expression) : node;
  const typeOf = node => session.type(unwrap(node));
  const arrayType = node => {
    const type = typeOf(node);
    return type && (checker.isArrayType(type) || checker.isTupleType(type));
  };
  function arrayIndex(node) {
    node = unwrap(node);
    if (!ts.isElementAccessExpression(node)) return false;
    const type = typeOf(node.expression);
    return type && (checker.isArrayType(type) || (checker.isTupleType(type) && !ts.isLiteralExpression(unwrap(node.argumentExpression))));
  }
  function optionalArrayIndex(node) {
    node = unwrap(node);
    if (!ts.isPropertyAccessExpression(node) && !ts.isElementAccessExpression(node) && !ts.isCallExpression(node)) return false;
    return !!(node.questionDotToken && arrayIndex(node.expression)) || optionalArrayIndex(node.expression);
  }
  const logical = node => ts.isBinaryExpression(node) && (node.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken || node.operatorToken.kind === ts.SyntaxKind.BarBarToken);
  function check(node, inverted = false) {
    node = unwrap(node);
    if (session.isReactive(node)) return;
    if (ts.isPrefixUnaryExpression(node) && node.operator === ts.SyntaxKind.ExclamationToken) {
      check(node.operand, true);
      return;
    }
    if (arrayIndex(node)) return;
    if (logical(node)) {
      check(node.right);
      return;
    }
    const type = typeOf(node);
    if (uncertain(type)) return;
    if (has(type, F.Never)) report(node, 'never');
    else if (!possiblyTruthy(type)) report(node, inverted ? 'alwaysTruthy' : 'alwaysFalsy');
    else if (!possiblyFalsy(type)) report(node, inverted ? 'alwaysFalsy' : 'alwaysTruthy');
  }
  function checkNullish(node) {
    node = unwrap(node);
    if (session.isReactive(node)) return;
    const type = typeOf(node);
    if (!type || has(type, F.Any | F.Unknown)) return;
    if (has(type, F.Never)) report(node, 'never');
    else if (!nullish(type)) {
      if (!arrayIndex(node) && !optionalArrayIndex(node)) report(node, 'neverNullish');
    } else if (parts(type).every(part => !!(part.flags & (F.Null | F.Undefined)))) report(node, 'alwaysNullish');
  }
  const comparisons = new Set(['<', '>', '<=', '>=', '==', '===', '!=', '!==']);
  function checkComparison(node) {
    if (session.isReactive(node)) return;
    const operator = node.operatorToken.getText(sourceFile);
    if (!comparisons.has(operator)) return;
    const left = typeOf(node.left), right = typeOf(node.right);
    if (!left || !right) return;
    if (literal(left) && literal(right)) {
      report(node, 'literalBooleanExpression');
      return;
    }
    if (!strictNull) return;
    const comparable = (type, flag) => has(type, flag | F.Any | F.Unknown | F.TypeParameter | ((operator === '==' || operator === '!=') ? F.Null | F.Undefined : 0));
    for (const flag of [F.Null, F.Undefined]) {
      if ((left.flags === flag && !comparable(right, flag)) || (right.flags === flag && !comparable(left, flag))) {
        report(node, 'noOverlapBooleanExpression');
        return;
      }
    }
  }
  function propertyNullable(objectType, propertyType, at) {
    return propertyType && parts(propertyType).some(type => {
      if (type.flags & (F.StringLiteral | F.NumberLiteral)) {
        const property = checker.getPropertyOfType(objectType, String(type.value));
        return property && nullish(checker.getTypeOfSymbolAtLocation(property, at));
      }
      return !!((type.flags & F.String) && checker.getIndexInfoOfType(objectType, ts.IndexKind.String)) || !!((type.flags & F.Number) && checker.getIndexInfoOfType(objectType, ts.IndexKind.Number));
    });
  }
  function nullableFromPrevious(node) {
    if (!ts.isPropertyAccessExpression(node) && !ts.isElementAccessExpression(node)) return false;
    const previous = typeOf(node.expression);
    if (!previous || !previous.isUnion()) return false;
    if (ts.isElementAccessExpression(node) && !ts.isIdentifier(unwrap(node.argumentExpression))) return false;
    const ownNullable = previous.types.some(type => {
      if (ts.isElementAccessExpression(node)) return propertyNullable(type, typeOf(node.argumentExpression), node);
      const property = checker.getPropertyOfType(type, node.name.text);
      return property && nullish(checker.getTypeOfSymbolAtLocation(property, node));
    });
    return !ownNullable && nullish(previous);
  }
  function checkOptional(node) {
    if (!node.questionDotToken || optionalArrayIndex(node)) return;
    const expression = unwrap(node.expression);
    if (session.isReactive(expression)) return;
    const type = typeOf(expression);
    if (!type || has(type, F.Any | F.Unknown) || (nullish(type) && !nullableFromPrevious(expression))) return;
    const range = session.range(node.questionDotToken);
    if (!range) return;
    const { start, end } = range;
    diagnostics.push({ message: messages.neverOptionalChain, start, end, fix: { start, end, text: ts.isPropertyAccessExpression(node) ? '.' : '' } });
  }
  const predicates = new Set(['filter', 'find', 'some', 'every']);
  function checkPredicate(node) {
    const callee = unwrap(node.expression);
    if (!ts.isPropertyAccessExpression(callee) || !predicates.has(callee.name.text) || !arrayType(callee.expression) || !node.arguments.length) return;
    const callback = unwrap(node.arguments[0]);
    if (ts.isArrowFunction(callback) || ts.isFunctionExpression(callback)) {
      if (!ts.isBlock(callback.body)) return check(callback.body);
      if (callback.body.statements.length === 1 && ts.isReturnStatement(callback.body.statements[0]) && callback.body.statements[0].expression) return check(callback.body.statements[0].expression);
    }
    const type = typeOf(callback);
    if (!type) return;
    const returns = type.getCallSignatures().map(signature => checker.getReturnTypeOfSignature(signature));
    if (!returns.length || returns.some(type => has(type, F.Any | F.Unknown))) return;
    if (!returns.some(possiblyFalsy)) report(callback, 'alwaysTruthyFunc');
    else if (!returns.some(possiblyTruthy)) report(callback, 'alwaysFalsyFunc');
  }
  function visit(node) {
    if (ts.isBinaryExpression(node)) {
      if (logical(node)) check(node.left);
      else if (node.operatorToken.kind === ts.SyntaxKind.QuestionQuestionToken) checkNullish(node.left);
      else checkComparison(node);
    } else if (ts.isIfStatement(node) || ts.isConditionalExpression(node)) check(ts.isIfStatement(node) ? node.expression : node.condition);
    else if (ts.isWhileStatement(node) || ts.isDoStatement(node) || ts.isForStatement(node)) {
      const condition = ts.isForStatement(node) ? node.condition : node.expression;
      if (condition) {
        const type = typeOf(condition);
        if (!(options.allowConstantLoopConditions && type && (type.flags & F.BooleanLiteral) && type.intrinsicName === 'true')) check(condition);
      }
    }
    if (ts.isCallExpression(node)) checkPredicate(node);
    if (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node) || ts.isCallExpression(node)) checkOptional(node);
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
  return diagnostics.sort((left, right) => left.start - right.start || left.end - right.end);
}

module.exports = { analyze };
