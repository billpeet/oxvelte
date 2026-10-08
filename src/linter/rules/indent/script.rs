//! JavaScript token relationships used by the Svelte indentation rule.
use super::layout::Layout;
use oxc::ast::{ast::*, AstKind};
use oxc::span::{GetSpan, Span};

fn mapped(span: Span, base: i64) -> Span {
    Span::new(
        (base + i64::from(span.start)) as u32,
        (base + i64::from(span.end)) as u32,
    )
}

fn find_after(l: &Layout<'_>, mut index: usize, value: &str) -> Option<usize> {
    while let Some(next) = l.after(index) {
        if l.text(next) == value {
            return Some(next);
        }
        index = next;
    }
    None
}
fn find_before(l: &Layout<'_>, mut index: usize, value: &str) -> Option<usize> {
    while let Some(next) = l.before(index) {
        if l.text(next) == value {
            return Some(next);
        }
        index = next;
    }
    None
}
fn next_non_paren(l: &Layout<'_>, mut index: usize) -> Option<usize> {
    while let Some(next) = l.after(index) {
        if l.text(next) != ")" {
            return Some(next);
        }
        index = next;
    }
    None
}
fn set_first(l: &mut Layout<'_>, span: Span, levels: i32, base: usize) {
    if let Some(first) = l.first(span) {
        l.set(first, levels, base);
    }
}
fn block(l: &mut Layout<'_>, span: Span, members: Vec<Span>) {
    let (Some(first), Some(last)) = (l.first(span), l.last(span)) else {
        return;
    };
    l.list(
        &members,
        l.tokens[first].span,
        Some(l.tokens[last].span),
        1,
        false,
    );
}
fn params(l: &mut Layout<'_>, p: &FormalParameters<'_>, base: i64, anchor: usize) {
    let span = mapped(p.span, base);
    let Some(first) = l.first(span) else {
        return;
    };
    if l.text(first) != "(" {
        return;
    }
    let Some(last) = l.last(span) else {
        return;
    };
    l.set(first, 1, anchor);
    let mut members: Vec<_> = p.items.iter().map(|x| mapped(x.span, base)).collect();
    if let Some(rest) = &p.rest {
        members.push(mapped(rest.span, base));
    }
    l.list(
        &members,
        l.tokens[first].span,
        Some(l.tokens[last].span),
        1,
        false,
    );
}
fn property(l: &mut Layout<'_>, span: Span, key: Span, value: Option<Span>, computed: bool) {
    let (Some(first), Some((kf, kl))) = (l.first(span), l.first_last(key, span.start)) else {
        return;
    };
    let mut current = first;
    while current < kf {
        if l.text(current) != "[" {
            l.set(current, 0, first);
        }
        let Some(next) = l.after(current) else {
            break;
        };
        current = next;
    }
    let last_key = if computed {
        let (Some(left), Some(right)) = (l.before(kf), l.after(kl)) else {
            return;
        };
        l.set(left, 0, first);
        l.list(
            &[key],
            l.tokens[left].span,
            Some(l.tokens[right].span),
            1,
            false,
        );
        right
    } else {
        l.set(kf, 0, first);
        kl
    };
    if let Some(v) = value {
        if let Some(vf) = l.first(v) {
            if vf <= last_key {
                l.set(vf, 1, last_key);
                return;
            }
            let mut current = last_key;
            while let Some(next) = l.after(current) {
                l.set(next, 1, last_key);
                if next >= vf {
                    break;
                }
                current = next;
            }
        }
    }
}
fn binary(
    l: &mut Layout<'_>,
    whole: Span,
    left: Span,
    right: Span,
    anchor_span: Span,
    assignment: bool,
) {
    let anchor = if assignment {
        l.first(anchor_span)
    } else {
        l.first_last(anchor_span, 0).map(|p| p.0)
    };
    let (Some(anchor), Some((lf, ll)), Some((rf, _))) = (
        anchor,
        l.first_last(left, whole.start),
        l.first_last(right, whole.start),
    ) else {
        return;
    };
    if lf != anchor {
        l.set(lf, 1, anchor);
    }
    if let Some(op) = next_non_paren(l, ll) {
        l.set(op, 1, anchor);
    }
    l.set(rf, 1, anchor);
}
fn chain(kind: AstKind<'_>) -> bool {
    if let AstKind::AssignmentTargetPropertyIdentifier(n) = kind {
        return n.init.is_some();
    }
    matches!(
        kind,
        AstKind::AssignmentExpression(_)
            | AstKind::AssignmentPattern(_)
            | AstKind::AssignmentTargetWithDefault(_)
            | AstKind::BinaryExpression(_)
            | AstKind::LogicalExpression(_)
    )
}
fn loop_body(l: &mut Layout<'_>, body: Span, anchor: usize) {
    if let Some(first) = l.first(body) {
        l.set(first, i32::from(l.text(first) != "{"), anchor);
    }
}
fn call(
    l: &mut Layout<'_>,
    whole: Span,
    callee: Span,
    arguments: Vec<Span>,
    types: Option<Span>,
    new: bool,
) {
    let (Some(first), Some((cf, cl)), Some(last)) = (
        l.first(whole),
        l.first_last(callee, whole.start),
        l.last(whole),
    ) else {
        return;
    };
    let anchor = if new {
        l.set(cf, 1, first);
        cf
    } else {
        first
    };
    if let Some(t) = types {
        set_first(l, t, 1, anchor);
    }
    let end = types.and_then(|t| l.last(t)).unwrap_or(cl);
    if let Some(left) = find_after(l, end, "(").filter(|x| *x <= last) {
        let mut current = end;
        while let Some(next) = l.after(current) {
            if next >= left {
                break;
            }
            if l.text(next) == "?." {
                l.set(next, 1, anchor);
            }
            current = next;
        }
        l.set(left, 1, anchor);
        l.list(
            &arguments,
            l.tokens[left].span,
            Some(l.tokens[last].span),
            1,
            false,
        );
    }
}

pub(super) fn apply_node(
    kind: AstKind<'_>,
    parent: Option<AstKind<'_>>,
    ancestors: &[AstKind<'_>],
    base: i64,
    l: &mut Layout<'_>,
) -> bool {
    let s = |span| mapped(span, base);
    let span = s(kind.span());
    let mut chain_head = kind;
    if chain(kind) {
        for ancestor in ancestors {
            if !chain(*ancestor) {
                break;
            }
            let current = s(chain_head.span());
            let parenthesized = l
                .first(current)
                .and_then(|i| l.before(i))
                .is_some_and(|i| l.text(i) == "(")
                && l.last(current)
                    .and_then(|i| l.after(i))
                    .is_some_and(|i| l.text(i) == ")");
            if parenthesized {
                break;
            }
            chain_head = *ancestor;
        }
    }
    let chain_span = s(chain_head.span());
    let assignment_chain = matches!(
        chain_head,
        AstKind::AssignmentExpression(_)
            | AstKind::AssignmentPattern(_)
            | AstKind::AssignmentTargetWithDefault(_)
            | AstKind::AssignmentTargetPropertyIdentifier(_)
    );
    let Some(first) = l.first(span) else {
        return true;
    };
    match kind {
        AstKind::Program(_) => {}
        AstKind::ArrayExpression(n) => {
            block(l, span, n.elements.iter().map(|x| s(x.span())).collect())
        }
        AstKind::ArrayPattern(n) => {
            let mut members: Vec<_> = n.elements.iter().flatten().map(|x| s(x.span())).collect();
            if let Some(r) = &n.rest {
                members.push(s(r.span));
            }
            block(l, span, members);
        }
        AstKind::ArrayAssignmentTarget(n) => {
            let mut members: Vec<_> = n.elements.iter().flatten().map(|x| s(x.span())).collect();
            if let Some(r) = &n.rest {
                members.push(s(r.span));
            }
            block(l, span, members);
        }
        AstKind::ObjectExpression(n) => {
            block(l, span, n.properties.iter().map(|x| s(x.span())).collect())
        }
        AstKind::ObjectPattern(n) => {
            let mut members: Vec<_> = n.properties.iter().map(|x| s(x.span())).collect();
            if let Some(r) = &n.rest {
                members.push(s(r.span));
            }
            block(l, span, members);
        }
        AstKind::ObjectAssignmentTarget(n) => {
            let mut members: Vec<_> = n.properties.iter().map(|x| s(x.span())).collect();
            if let Some(r) = &n.rest {
                members.push(s(r.span));
            }
            block(l, span, members);
        }
        AstKind::BlockStatement(n) => block(l, span, n.body.iter().map(|x| s(x.span())).collect()),
        AstKind::FunctionBody(n) => {
            if l.text(first) == "{" {
                let mut members: Vec<_> = n.directives.iter().map(|x| s(x.span)).collect();
                members.extend(n.statements.iter().map(|x| s(x.span())));
                block(l, span, members);
            }
        }
        AstKind::ClassBody(n) => block(l, span, n.body.iter().map(|x| s(x.span())).collect()),
        AstKind::ArrowFunctionExpression(n) => {
            if n.r#async {
                if let Some(second) = l.after(first) {
                    l.set(second, 1, first);
                }
            }
            params(l, &n.params, base, first);
            let body = s(n.body.span);
            if let Some(bf) = l.first(body) {
                if let Some(arrow) = find_before(l, bf, "=>") {
                    l.set(arrow, 1, first);
                }
                loop_body(l, body, first);
            }
        }
        AstKind::Function(n) => {
            let anchor = if matches!(
                parent,
                Some(AstKind::MethodDefinition(_) | AstKind::ObjectProperty(_))
            ) {
                parent.and_then(|p| l.first(s(p.span()))).unwrap_or(first)
            } else {
                first
            };
            if let Some(pf) = l.first(s(n.params.span)) {
                let mut current = first;
                let mut offset = 0;
                while let Some(next) = l.after(current) {
                    if next >= pf || l.text(next) == "<" {
                        break;
                    }
                    if l.text(next) == "*"
                        || n.id
                            .as_ref()
                            .is_some_and(|id| s(id.span).start == l.tokens[next].span.start)
                    {
                        offset = 1;
                    }
                    l.set(next, offset, first);
                    current = next;
                }
            }
            params(l, &n.params, base, anchor);
            if let Some(b) = &n.body {
                set_first(l, s(b.span), 0, anchor);
            }
        }
        AstKind::BinaryExpression(n) => binary(
            l,
            span,
            s(n.left.span()),
            s(n.right.span()),
            chain_span,
            assignment_chain,
        ),
        AstKind::LogicalExpression(n) => binary(
            l,
            span,
            s(n.left.span()),
            s(n.right.span()),
            chain_span,
            assignment_chain,
        ),
        AstKind::AssignmentExpression(n) => binary(
            l,
            span,
            s(n.left.span()),
            s(n.right.span()),
            chain_span,
            assignment_chain,
        ),
        AstKind::AssignmentPattern(n) => binary(
            l,
            span,
            s(n.left.span()),
            s(n.right.span()),
            chain_span,
            assignment_chain,
        ),
        AstKind::AssignmentTargetWithDefault(n) => binary(
            l,
            span,
            s(n.binding.span()),
            s(n.init.span()),
            chain_span,
            assignment_chain,
        ),
        AstKind::AwaitExpression(_)
        | AstKind::UnaryExpression(_)
        | AstKind::SpreadElement(_)
        | AstKind::BindingRestElement(_)
        | AstKind::AssignmentTargetRest(_)
        | AstKind::FormalParameterRest(_)
        | AstKind::UpdateExpression(_) => {
            if let Some(next) = l.after(first) {
                l.set(next, 1, first);
            }
        }
        AstKind::CallExpression(n) => call(
            l,
            span,
            s(n.callee.span()),
            n.arguments.iter().map(|x| s(x.span())).collect(),
            n.type_arguments.as_ref().map(|x| s(x.span)),
            false,
        ),
        AstKind::NewExpression(n) => call(
            l,
            span,
            s(n.callee.span()),
            n.arguments.iter().map(|x| s(x.span())).collect(),
            n.type_arguments.as_ref().map(|x| s(x.span)),
            true,
        ),
        AstKind::VariableDeclaration(n) => l.list(
            &n.declarations.iter().map(|x| s(x.span)).collect::<Vec<_>>(),
            l.tokens[first].span,
            None,
            1,
            false,
        ),
        AstKind::VariableDeclarator(n) => {
            if let Some(init) = &n.init {
                if let Some((init_first, _)) = l.first_last(s(init.span()), span.start) {
                    if let Some(eq) = find_before(l, init_first, "=").filter(|i| *i >= first) {
                        l.set(eq, 1, first);
                        if let Some(next) = l.after(eq) {
                            l.set(next, 1, first);
                        }
                    }
                }
            }
        }
        AstKind::ObjectProperty(n) => property(
            l,
            span,
            s(n.key.span()),
            Some(s(n.value.span())),
            n.computed,
        ),
        AstKind::AssignmentTargetPropertyProperty(n) => property(
            l,
            span,
            s(n.name.span()),
            Some(s(n.binding.span())),
            n.computed,
        ),
        AstKind::AssignmentTargetPropertyIdentifier(n) => {
            if let Some(init) = &n.init {
                binary(l, span, s(n.binding.span), s(init.span()), span, true);
            }
        }
        AstKind::BindingProperty(n) => property(
            l,
            span,
            s(n.key.span()),
            Some(s(n.value.span())),
            n.computed,
        ),
        AstKind::MethodDefinition(n) => {
            property(l, span, s(n.key.span()), Some(s(n.value.span)), n.computed)
        }
        AstKind::PropertyDefinition(n) => property(
            l,
            span,
            s(n.key.span()),
            n.value.as_ref().map(|x| s(x.span())),
            n.computed,
        ),
        AstKind::ComputedMemberExpression(n) => {
            if let Some(pf) = l.first(s(n.expression.span())) {
                if let Some(left) = find_before(l, pf, "[") {
                    if let Some(ol) = l.last(s(n.object.span())) {
                        let mut current = ol;
                        while let Some(next) = l.after(current) {
                            if next >= left {
                                break;
                            }
                            if l.text(next) == "?." {
                                l.set(next, 1, first);
                            }
                            current = next;
                        }
                    }
                    l.set(left, 1, first);
                    if let Some(pl) = l.last(s(n.expression.span())) {
                        if let Some(right) = find_after(l, pl, "]") {
                            l.list(
                                &[s(n.expression.span())],
                                l.tokens[left].span,
                                Some(l.tokens[right].span),
                                1,
                                false,
                            );
                        }
                    }
                }
            }
        }
        AstKind::StaticMemberExpression(n) => {
            if let Some(pf) = l.first(s(n.property.span)) {
                if let Some(dot) = l.before(pf) {
                    l.set(dot, 1, first);
                }
                l.set(pf, 1, first);
            }
        }
        AstKind::PrivateFieldExpression(n) => {
            if let Some(pf) = l.first(s(n.field.span)) {
                if let Some(dot) = l.before(pf) {
                    l.set(dot, 1, first);
                }
                l.set(pf, 1, first);
            }
        }
        AstKind::MetaProperty(n) => {
            if let Some(pf) = l.first(s(n.property.span)) {
                if let Some(dot) = l.before(pf) {
                    l.set(dot, 1, first);
                }
                l.set(pf, 1, first);
            }
        }
        AstKind::ConditionalExpression(n) => {
            let mut conditional_head = n;
            for ancestor in ancestors {
                if let AstKind::ConditionalExpression(p) = ancestor {
                    if p.alternate.span() == conditional_head.span {
                        conditional_head = p;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            let first = l.first(s(conditional_head.span)).unwrap_or(first);
            if let Some(testlast) = l.last(s(n.test.span())) {
                if let Some(question) = next_non_paren(l, testlast) {
                    l.set(question, 1, first);
                    set_first(l, s(n.consequent.span()), 1, question);
                }
            }
            if let Some(last) = l.last(s(n.consequent.span())) {
                if let Some(colon) = next_non_paren(l, last) {
                    l.set(colon, 1, first);
                    set_first(l, s(n.alternate.span()), 1, colon);
                }
            }
        }
        AstKind::IfStatement(n) => {
            if let Some(left) = l.after(first) {
                l.set(left, 1, first);
                if let Some(body) = l.first(s(n.consequent.span())) {
                    if let Some(right) = find_before(l, body, ")") {
                        l.set(right, 0, left);
                    }
                }
            }
            loop_body(l, s(n.consequent.span()), first);
            if let Some(alt) = &n.alternate {
                if let Some(cl) = l.last(s(n.consequent.span())) {
                    if let Some(else_token) = next_non_paren(l, cl) {
                        l.set(else_token, 0, first);
                        loop_body(l, s(alt.span()), else_token);
                    }
                }
            }
        }
        AstKind::WhileStatement(n) => control_test(l, first, s(n.test.span()), s(n.body.span())),
        AstKind::WithStatement(n) => control_test(l, first, s(n.object.span()), s(n.body.span())),
        AstKind::DoWhileStatement(n) => {
            loop_body(l, s(n.body.span()), first);
            if let Some(bl) = l.last(s(n.body.span())) {
                if let Some(w) = next_non_paren(l, bl) {
                    l.set(w, 0, first);
                    if let Some(left) = l.after(w) {
                        l.set(left, 1, w);
                        if let Some(tl) = l.last(s(n.test.span())) {
                            if let Some(right) = l.after(tl) {
                                l.list(
                                    &[s(n.test.span())],
                                    l.tokens[left].span,
                                    Some(l.tokens[right].span),
                                    1,
                                    false,
                                );
                            }
                        }
                    }
                }
            }
        }
        AstKind::ForStatement(n) => {
            if let Some(left) = l.after(first) {
                l.set(left, 1, first);
                if let Some(bf) = l.first(s(n.body.span())) {
                    if let Some(right) = find_before(l, bf, ")") {
                        let members: Vec<_> = n
                            .init
                            .iter()
                            .map(|x| s(x.span()))
                            .chain(n.test.iter().map(|x| s(x.span())))
                            .chain(n.update.iter().map(|x| s(x.span())))
                            .collect();
                        l.list(
                            &members,
                            l.tokens[left].span,
                            Some(l.tokens[right].span),
                            1,
                            false,
                        );
                    }
                }
            }
            loop_body(l, s(n.body.span()), first);
        }
        AstKind::ForInStatement(n) => for_in(
            l,
            first,
            s(n.left.span()),
            s(n.right.span()),
            s(n.body.span()),
            false,
        ),
        AstKind::ForOfStatement(n) => for_in(
            l,
            first,
            s(n.left.span()),
            s(n.right.span()),
            s(n.body.span()),
            n.r#await,
        ),
        AstKind::LabeledStatement(_) => {
            if let Some(colon) = l.after(first) {
                l.set(colon, 1, first);
                if let Some(body) = l.after(colon) {
                    l.set(body, 1, first);
                }
            }
        }
        AstKind::ReturnStatement(n) => {
            if n.argument.is_some() {
                if let Some(next) = l.after(first) {
                    l.set(next, 1, first);
                }
            }
        }
        AstKind::ThrowStatement(_) => {
            if let Some(next) = l.after(first) {
                l.set(next, 1, first);
            }
        }
        AstKind::BreakStatement(n) => {
            if n.label.is_some() {
                if let Some(next) = l.after(first) {
                    l.set(next, 1, first);
                }
            }
        }
        AstKind::ContinueStatement(n) => {
            if n.label.is_some() {
                if let Some(next) = l.after(first) {
                    l.set(next, 1, first);
                }
            }
        }
        AstKind::YieldExpression(n) => {
            if n.argument.is_some() {
                if let Some(next) = l.after(first) {
                    l.set(next, 1, first);
                    if n.delegate {
                        if let Some(argument) = l.after(next) {
                            l.set(argument, 1, first);
                        }
                    }
                }
            }
        }
        AstKind::SequenceExpression(n) => l.list(
            &n.expressions
                .iter()
                .map(|x| s(x.span()))
                .collect::<Vec<_>>(),
            l.tokens[first].span,
            None,
            0,
            false,
        ),
        AstKind::SwitchStatement(n) => {
            if let Some((left, right)) = l.first_last(s(n.discriminant.span()), span.start) {
                l.set(left, 1, first);
                l.list(
                    &[s(n.discriminant.span())],
                    l.tokens[left].span,
                    Some(l.tokens[right].span),
                    1,
                    false,
                );
                if let Some(brace) = l.after(right) {
                    l.set(brace, 0, first);
                    if let Some(last) = l.last(span) {
                        let offset = l.options.switch_case;
                        l.list(
                            &n.cases.iter().map(|x| s(x.span)).collect::<Vec<_>>(),
                            l.tokens[brace].span,
                            Some(l.tokens[last].span),
                            offset,
                            false,
                        );
                    }
                }
            }
        }
        AstKind::SwitchCase(n) => {
            let testlast = n
                .test
                .as_ref()
                .and_then(|t| l.first_last(s(t.span()), span.start));
            if let Some((tf, tl)) = testlast {
                l.set(tf, 1, first);
                if let Some(colon) = l.after(tl) {
                    l.set(colon, 1, first);
                }
            } else if let Some(colon) = l.after(first) {
                l.set(colon, 1, first);
            }
            let block_only =
                n.consequent.len() == 1 && matches!(n.consequent[0], Statement::BlockStatement(_));
            for st in &n.consequent {
                if let Some((sf, _)) = l.first_last(s(st.span()), span.start) {
                    l.set(sf, i32::from(!block_only), first);
                }
            }
        }
        AstKind::TryStatement(n) => {
            set_first(l, s(n.block.span), 0, first);
            if let Some(h) = &n.handler {
                set_first(l, s(h.span), 0, first);
            }
            if let Some(f) = &n.finalizer {
                if let Some(ff) = l.first(s(f.span)) {
                    if let Some(ft) = l.before(ff) {
                        l.set(ft, 0, first);
                    }
                    l.set(ff, 0, first);
                }
            }
        }
        AstKind::CatchClause(n) => {
            if let Some(p) = &n.param {
                if let Some(pf) = l.first(s(p.span)) {
                    if let Some(left) = l.before(pf) {
                        l.set(left, 1, first);
                        if let Some(pl) = l.last(s(p.span)) {
                            if let Some(right) = l.after(pl) {
                                l.list(
                                    &[s(p.span)],
                                    l.tokens[left].span,
                                    Some(l.tokens[right].span),
                                    1,
                                    false,
                                );
                            }
                        }
                    }
                }
            }
            set_first(l, s(n.body.span), 0, first);
        }
        AstKind::Class(n) => {
            if let Some(id) = &n.id {
                set_first(l, s(id.span), 1, first);
            }
            if let Some(superclass) = &n.super_class {
                if let Some(sf) = l.first(s(superclass.span())) {
                    if let Some(ext) = l.before(sf) {
                        l.set(ext, 1, first);
                        l.set(sf, 1, ext);
                    }
                }
            }
            set_first(l, s(n.body.span), 0, first);
        }
        AstKind::TaggedTemplateExpression(n) => {
            if let Some((tf, _)) = l.first_last(s(n.tag.span()), span.start) {
                set_first(l, s(n.quasi.span), 1, tf);
            }
        }
        AstKind::TemplateLiteral(n) => {
            for q in n.quasis.iter().skip(1) {
                let qspan = s(q.span);
                if let Some(index) = l
                    .tokens
                    .iter()
                    .position(|t| t.span.start <= qspan.start && qspan.start < t.span.end)
                {
                    l.set(index, 0, first);
                }
            }
            for e in &n.expressions {
                set_first(l, s(e.span()), 1, first);
            }
        }
        AstKind::ParenthesizedExpression(n) => {
            if let Some(last) = l.last(span) {
                set_first(l, s(n.expression.span()), 1, first);
                l.set(last, 0, first);
            }
        }
        AstKind::ImportExpression(n) => {
            if let Some(left) = find_after(l, first, "(") {
                l.set(left, 1, first);
                if let Some(last) = l.last(span) {
                    l.list(
                        &[s(n.source.span())],
                        l.tokens[left].span,
                        Some(l.tokens[last].span),
                        1,
                        false,
                    );
                }
            }
        }
        AstKind::ImportDeclaration(n) => import_export(
            l,
            span,
            Some(s(n.source.span)),
            n.specifiers
                .as_ref()
                .map(|specs| {
                    specs
                        .iter()
                        .filter_map(|x| {
                            if matches!(x, ImportDeclarationSpecifier::ImportSpecifier(_)) {
                                Some(s(x.span()))
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            false,
        ),
        AstKind::ExportNamedDeclaration(n) => {
            if let Some(d) = &n.declaration {
                set_first(l, s(d.span()), 1, first);
            } else {
                import_export(
                    l,
                    span,
                    n.source.as_ref().map(|x| s(x.span)),
                    n.specifiers.iter().map(|x| s(x.span)).collect(),
                    true,
                );
            }
        }
        AstKind::ExportAllDeclaration(n) => {
            import_export(l, span, Some(s(n.source.span)), Vec::new(), true)
        }
        AstKind::ExportDefaultDeclaration(n) => {
            if let Some(df) = l.first(s(n.declaration.span())) {
                let mut current = first;
                while let Some(next) = l.after(current) {
                    l.set(next, 1, first);
                    if next >= df {
                        break;
                    }
                    current = next;
                }
            }
        }
        AstKind::ImportSpecifier(_)
        | AstKind::ExportSpecifier(_)
        | AstKind::ImportNamespaceSpecifier(_) => {
            let mut anchor = first;
            if l.text(first) == "type" {
                if let Some(next) = l.after(first) {
                    l.set(next, 0, first);
                    anchor = next;
                }
            }
            if let Some(last) = l.last(span) {
                let mut current = anchor;
                while let Some(next) = l.after(current) {
                    if next > last {
                        break;
                    }
                    l.set(next, 1, anchor);
                    current = next;
                }
            }
        }
        AstKind::IdentifierName(_)
        | AstKind::IdentifierReference(_)
        | AstKind::BindingIdentifier(_)
        | AstKind::LabelIdentifier(_)
        | AstKind::ThisExpression(_)
        | AstKind::Super(_)
        | AstKind::TemplateElement(_)
        | AstKind::Elision(_)
        | AstKind::BooleanLiteral(_)
        | AstKind::NullLiteral(_)
        | AstKind::NumericLiteral(_)
        | AstKind::StringLiteral(_)
        | AstKind::BigIntLiteral(_)
        | AstKind::RegExpLiteral(_)
        | AstKind::PrivateIdentifier(_)
        | AstKind::ImportDefaultSpecifier(_)
        | AstKind::DebuggerStatement(_)
        | AstKind::EmptyStatement(_)
        | AstKind::ExpressionStatement(_)
        | AstKind::ChainExpression(_)
        | AstKind::FormalParameters(_)
        | AstKind::FormalParameter(_)
        | AstKind::CatchParameter(_)
        | AstKind::Directive(_)
        | AstKind::WithClause(_) => {}
        _ => return false,
    }
    // ESTree treats grammar parentheses around conditions like expression parentheses.
    // OXC records explicit parentheses as nodes, but omits the condition delimiters.
    if is_expression(kind) {
        let mut inner = first;
        let mut last = l.last(span).unwrap_or(first);
        while let (Some(left), Some(right)) = (l.before(inner), l.after(last)) {
            if l.text(left) != "(" || l.text(right) != ")" {
                break;
            }
            l.set(inner, 1, left);
            l.set(right, 0, left);
            inner = left;
            last = right;
        }
    }
    if let Some(last) = l.last(span) {
        if !matches!(kind,AstKind::Function(n) if n.body.is_none())
            && l.text(last) == ";"
            && last != first
        {
            if l.after(last).is_none_or(|next| {
                l.line_of(l.tokens[last].span.start) < l.line_of(l.tokens[next].span.start)
            }) {
                l.set(last, 0, first);
            }
        }
    }
    true
}

fn is_expression(kind: AstKind<'_>) -> bool {
    matches!(
        kind,
        AstKind::IdentifierReference(_)
            | AstKind::ThisExpression(_)
            | AstKind::ArrayExpression(_)
            | AstKind::ObjectExpression(_)
            | AstKind::TemplateLiteral(_)
            | AstKind::TaggedTemplateExpression(_)
            | AstKind::ComputedMemberExpression(_)
            | AstKind::StaticMemberExpression(_)
            | AstKind::PrivateFieldExpression(_)
            | AstKind::CallExpression(_)
            | AstKind::NewExpression(_)
            | AstKind::MetaProperty(_)
            | AstKind::UpdateExpression(_)
            | AstKind::UnaryExpression(_)
            | AstKind::BinaryExpression(_)
            | AstKind::LogicalExpression(_)
            | AstKind::ConditionalExpression(_)
            | AstKind::AssignmentExpression(_)
            | AstKind::SequenceExpression(_)
            | AstKind::Super(_)
            | AstKind::AwaitExpression(_)
            | AstKind::ChainExpression(_)
            | AstKind::ParenthesizedExpression(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::YieldExpression(_)
            | AstKind::ImportExpression(_)
            | AstKind::BooleanLiteral(_)
            | AstKind::NullLiteral(_)
            | AstKind::NumericLiteral(_)
            | AstKind::StringLiteral(_)
            | AstKind::BigIntLiteral(_)
            | AstKind::RegExpLiteral(_)
    )
}

fn control_test(l: &mut Layout<'_>, first: usize, _test: Span, body: Span) {
    if let Some(left) = l.after(first) {
        l.set(left, 1, first);
        if let Some(bf) = l.first(body) {
            if let Some(right) = find_before(l, bf, ")") {
                l.set(right, 0, left);
            }
        }
    }
    loop_body(l, body, first);
}
fn for_in(
    l: &mut Layout<'_>,
    first: usize,
    left_span: Span,
    right_span: Span,
    body: Span,
    is_await: bool,
) {
    let Some(mut left) = l.after(first) else {
        return;
    };
    if is_await {
        l.set(left, 0, first);
        let Some(next) = l.after(left) else {
            return;
        };
        left = next;
    }
    l.set(left, 1, first);
    if let Some(lf) = l.first(left_span) {
        l.set(lf, 1, left);
        if let Some(ll) = l.last(left_span) {
            if let Some(op) = next_non_paren(l, ll) {
                l.set(op, 1, lf);
                set_first(l, right_span, 1, lf);
            }
        }
    }
    if let Some(bf) = l.first(body) {
        if let Some(right) = find_before(l, bf, ")") {
            l.set(right, 0, left);
        }
    }
    loop_body(l, body, first);
}
fn import_export(
    l: &mut Layout<'_>,
    span: Span,
    source: Option<Span>,
    specs: Vec<Span>,
    export: bool,
) {
    let Some(first) = l.first(span) else {
        return;
    };
    let Some(last) = l.last(span) else {
        return;
    };
    let from = source
        .and_then(|src| l.first(src))
        .and_then(|sf| find_before(l, sf, "from"))
        .filter(|x| *x > first);
    let end = from.unwrap_or_else(|| source.and_then(|src| l.first(src)).unwrap_or(last));
    let mut before = Vec::new();
    let mut current = first;
    while let Some(next) = l.after(current) {
        if next >= end {
            break;
        }
        before.push(next);
        current = next;
    }
    if let Some(sf) = specs.first().and_then(|s| l.first(*s)) {
        if let Some(left) = l.before(sf) {
            if let Some(sl) = specs.last().and_then(|s| l.last(*s)) {
                if let Some(right) = find_after(l, sl, "}") {
                    l.list(
                        &specs,
                        l.tokens[left].span,
                        Some(l.tokens[right].span),
                        1,
                        false,
                    );
                    before.retain(|x| *x <= left || *x > right);
                }
            }
        }
    }
    if specs.is_empty() {
        if let Some(left) = before.iter().copied().find(|x| l.text(*x) == "{") {
            if let Some(right) = find_after(l, left, "}").filter(|x| *x <= last) {
                l.set(left, 0, first);
                l.list(
                    &[],
                    l.tokens[left].span,
                    Some(l.tokens[right].span),
                    1,
                    false,
                );
                before.retain(|x| *x <= left || *x > right);
            }
        }
    }
    let braces_only = before.iter().all(|x| matches!(l.text(*x), "{" | "}"));
    let offset = if braces_only { 0 } else { 1 };
    let mut anchor = first;
    for token in before {
        if export && l.text(token) == "as" {
            anchor = l.before(token).unwrap_or(first);
        }
        l.set(token, offset, anchor);
    }
    if let Some(from) = from {
        l.set(from, 0, first);
        if let Some(src) = source {
            set_first(l, src, 1, from);
        }
    } else if let Some(src) = source {
        set_first(l, src, 1, first);
    }
    if let Some(src_last) = source.and_then(|s| l.last(s)) {
        if let Some(assert) = l.after(src_last).filter(|x| *x < last) {
            l.set(assert, 0, first);
            if let Some(open) = l.after(assert).filter(|x| l.text(*x) == "{") {
                l.set(open, 1, assert);
                if let Some(close) = find_after(l, open, "}") {
                    let mut members = Vec::new();
                    let mut current = open;
                    while let Some(next) = l.after(current) {
                        if next >= close {
                            break;
                        }
                        members.push(l.tokens[next].span);
                        current = next;
                    }
                    l.list(
                        &members,
                        l.tokens[open].span,
                        Some(l.tokens[close].span),
                        1,
                        false,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        linter::{Linter, RuleConfig},
        parser,
    };
    use oxc::allocator::Allocator;

    fn fixed(source: &str, options: serde_json::Value) -> String {
        let alloc = Allocator::default();
        let parsed = parser::parse_for_lint(source, &alloc);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let mut fixes: Vec<_> = Linter::all()
            .lint_with_config_and_path(
                &parsed.ast,
                source,
                RuleConfig {
                    options: Some(options),
                    settings: None,
                },
                "Regression.svelte",
            )
            .into_iter()
            .filter(|d| d.rule_name == "svelte/indent")
            .filter_map(|d| d.fix)
            .collect();
        fixes.sort_by_key(|f| std::cmp::Reverse(f.span.start));
        let mut result = source.to_owned();
        for fix in fixes {
            result.replace_range(
                fix.span.start as usize..fix.span.end as usize,
                &fix.replacement,
            );
        }
        result
    }

    #[test]
    fn script_chains_keep_original_unicode_and_crlf_offsets() {
        let source = "<!-- é😀 -->\r\n<script data-note=\">\">\r\nfunction f() {\r\nconst result = a\r\n+ b\r\n+ c;\r\nreturn result;\r\n}\r\n</script>";
        let expected = "<!-- é😀 -->\r\n<script data-note=\">\">\r\n  function f() {\r\n    const result = a\r\n      + b\r\n      + c;\r\n    return result;\r\n  }\r\n</script>";
        assert_eq!(fixed(source, serde_json::json!([2])), expected);
        assert_eq!(fixed(expected, serde_json::json!([2])), expected);
    }

    #[test]
    fn script_tab_offsets_honor_the_script_anchor_option() {
        let source = "<script>\nfunction f() {\nconst value = first\n+ second\n+ third;\nreturn value;\n}\n</script>";
        let expected = "<script>\nfunction f() {\n\tconst value = first\n\t\t+ second\n\t\t+ third;\n\treturn value;\n}\n</script>";
        let options = serde_json::json!([{"indent": "tab", "indentScript": false}]);
        assert_eq!(fixed(source, options.clone()), expected);
        assert_eq!(fixed(expected, options), expected);
    }
}
