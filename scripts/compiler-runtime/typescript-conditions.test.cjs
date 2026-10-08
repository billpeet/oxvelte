'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const ts = require(require.resolve('typescript', { paths: [process.env.OXVELTE_COMPILER_RUNTIME || __dirname] }));
const { analyze } = require('./typescript-conditions.cjs');
const { createSession } = require('./typescript-service.cjs');

// A real checker isolates the expression handlers from the Svelte transport.
function run(source, options = {}, compilerOptions = {}) {
  const filename = path.resolve('conditions-test.ts').replaceAll('\\', '/');
  const config = { strict: true, target: ts.ScriptTarget.Latest, ...compilerOptions };
  const host = ts.createCompilerHost(config);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (name, languageVersion, ...rest) => name === filename ? ts.createSourceFile(name, source, languageVersion, true) : original(name, languageVersion, ...rest);
  const program = ts.createProgram([filename], config, host);
  const checker = program.getTypeChecker();
  const sourceFile = program.getSourceFile(filename);
  return analyze({ ts, checker, sourceFile, source, compilerOptions: config,
    isReactive: () => false,
    type(node) { const type = checker.getTypeAtLocation(node); return checker.getBaseConstraintOfType(type) || type; },
    range(node) { return { start: node.getStart(sourceFile), end: node.end }; },
  }, options);
}
const snippets = (source, diagnostics) => diagnostics.map(item => [source.slice(item.start, item.end), item.message]);

test('truthiness uses flow-sensitive union and literal types, with unary inversion', () => {
  const source = 'let value: string | null = null; value || 4; let flag = false; !flag && 1; if ("yes") {} if (0n) {}';
  assert.deepEqual(snippets(source, run(source)), [
    ['value', 'Unnecessary conditional, value is always falsy.'],
    ['flag', 'Unnecessary conditional, value is always truthy.'],
    ['"yes"', 'Unnecessary conditional, value is always truthy.'],
    ['0n', 'Unnecessary conditional, value is always falsy.'],
  ]);
});

test('literal comparisons, disjoint null types and nullish operands', () => {
  const source = 'let value: string | null = null; value == null; value ?? 2; declare const object: {}; object === null; object ?? 4;';
  assert.deepEqual(snippets(source, run(source)), [
    ['value == null', 'Unnecessary conditional, both sides of the expression are literal values.'],
    ['value', 'Unnecessary conditional, left-hand side of `??` operator is always `null` or `undefined`.'],
    ['object === null', 'Unnecessary conditional, the types have no overlap.'],
    ['object', 'Unnecessary conditional, expected left-hand side of `??` operator to be possibly null or undefined.'],
  ]);
});

test('optional property, computed and call operators preserve exact fixes', () => {
  const source = 'declare const object: {x: string}; object?.x; object?.["x"]; declare const fn: () => void; fn?.();';
  const diagnostics = run(source);
  assert.deepEqual(diagnostics.map(item => source.slice(item.start, item.end)), ['?.', '?.', '?.']);
  assert.deepEqual(diagnostics.map(item => item.fix.text), ['.', '', '']);
});

test('nullable optional origins report only the redundant subsequent step', () => {
  const source = 'declare const object: {x: {y: number}} | null; object?.x?.y; declare const own: {x?: {y: number}} | null; own?.x?.y;';
  const diagnostics = run(source);
  assert.equal(diagnostics.length, 1);
  assert.equal(diagnostics[0].start, source.indexOf('?.y'));
});

test('any, unknown, generic parameters and array bounds remain conditional', () => {
  const source = 'declare const a: any; declare const b: unknown; if(a){} if(b){} function f<T>(x:T){if(x){}} declare const arr: string[]; if(arr[0]){} arr[0] ?? "x"; arr[0]?.length; arr[0]?.toString()?.length;';
  assert.deepEqual(run(source), []);
});

test('tuple literal indices remain checkable and never is reported', () => {
  const source = 'declare const tuple: ["yes"]; if(tuple[0]){} declare const impossible: never; if(impossible){}';
  assert.deepEqual(snippets(source, run(source)).map(pair => pair[1]), [
    'Unnecessary conditional, value is always truthy.',
    'Unnecessary conditional, value is `never`.',
  ]);
});

test('loops and strict-null-check options follow upstream behavior', () => {
  const source = 'while(true){} while(false){} for(;true;){}';
  assert.equal(run(source).length, 3);
  assert.deepEqual(snippets(source, run(source, { allowConstantLoopConditions: true })).map(pair => pair[0]), ['false']);
  assert.equal(run('', {}, { strict: false })[0].message, 'This rule requires the `strictNullChecks` compiler option to be turned on to function correctly.');
  assert.deepEqual(run('', { allowRuleToRunWithoutStrictNullChecksIKnowWhatIAmDoing: true }, { strict: false }), []);
});

test('array predicates check inline returns and callable result signatures', () => {
  const source = 'const arr = [1]; arr.filter(x => 1); arr.find(x => {return false;}); declare const predicate: () => {x:number}; arr.some(predicate);';
  assert.deepEqual(snippets(source, run(source)).map(pair => pair[1]), [
    'Unnecessary conditional, value is always truthy.',
    'Unnecessary conditional, value is always falsy.',
    'This callback should return a conditional, but return is always truthy.',
  ]);
});

test('Svelte reactive exclusions preserve locals, immutable roots and ordinary narrowing', () => {
  const script = 'let value: string | null = null; value ?? 1; $: value ?? 2; $: {let value: string | null = null; value ?? 3;} const fixed = null; $: fixed ?? 4;';
  const source = `<script lang="ts">${script}</script>`;
  const start = source.indexOf('>') + 1;
  const diagnostics = analyze(createSession({ source, filename: path.resolve('__conditions__.svelte'), scripts: [{start, end:start + script.length}] }));
  assert.deepEqual(diagnostics.map(item => source.slice(item.start, item.end)), ['value', 'value', 'fixed']);
  assert.deepEqual(diagnostics.map(item => item.start), [source.indexOf('value ?? 1'), source.indexOf('value ?? 3'), source.indexOf('fixed ?? 4')]);
});

test('template expressions map operator fixes and distinguish mutable bindings', () => {
  const script = 'let mutable = null; const fixed = null; const object = {x: 1};';
  const source = `<script>${script}</script>😀{#if mutable}x{/if}{#if fixed}y{/if}{object?.x}`;
  const templateStart = source.indexOf('</script>') + 9;
  const expression = (text, kind) => { const start = source.indexOf(text, templateStart); return {start, end:start + text.length, kind}; };
  const diagnostics = analyze(createSession({ source, filename: path.resolve('__conditions__.svelte'), scripts: [{start:8, end:8+script.length}], templates: [expression('mutable', 'condition'), expression('fixed', 'condition'), expression('object?.x', 'expression')] }));
  assert.deepEqual(diagnostics.map(item => source.slice(item.start, item.end)), ['fixed', '?.']);
  assert.deepEqual(diagnostics[1].fix, {start:source.indexOf('?.'), end:source.indexOf('?.')+2, text:'.'});
});
