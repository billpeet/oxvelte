//! TypeScript token relationships, following the upstream indentation visitor.
use super::Layout;
use oxc::ast::{ast::*, AstKind};
use oxc::span::{GetSpan, Span};

fn shifted(span: Span, base: i64) -> Span {
    Span::new(
        (i64::from(span.start) + base) as u32,
        (i64::from(span.end) + base) as u32,
    )
}

fn tokens(layout: &Layout, span: Span) -> Vec<usize> {
    layout
        .tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            (!t.comment && t.span.start >= span.start && t.span.end <= span.end).then_some(i)
        })
        .collect()
}

fn range(layout: &mut Layout, from: usize, to: usize, offset: i32, anchor: usize) {
    for i in from..=to {
        if !layout.tokens[i].comment {
            layout.set(i, offset, anchor);
        }
    }
}

fn child(layout: &mut Layout, span: Span, offset: i32, anchor: usize) {
    if let Some(i) = layout.first(span) {
        layout.set(i, offset, anchor);
    }
}

fn list(layout: &mut Layout, spans: impl IntoIterator<Item = Span>, span: Span, base: i64) {
    let spans: Vec<_> = spans.into_iter().map(|s| shifted(s, base)).collect();
    let span = shifted(span, base);
    if let (Some(first), Some(last)) = (layout.first(span), layout.last(span)) {
        layout.list(
            &spans,
            layout.tokens[first].span,
            Some(layout.tokens[last].span),
            1,
            false,
        );
    }
}

fn suffix(layout: &mut Layout, span: Span, count: usize, offset: i32) {
    let ts = tokens(layout, span);
    if let Some(&first) = ts.first() {
        for &i in ts.iter().rev().take(count) {
            layout.set(i, offset, first);
        }
    }
}

fn binary_type(layout: &mut Layout, expr: Span, ty: Span) {
    if let Some((first, last)) = layout.first_last(expr, 0) {
        if let Some(op) = layout.after(last) {
            layout.set(op, 1, first);
        }
        child(layout, ty, 1, first);
    }
}

fn generic(layout: &mut Layout, span: Span, arguments: Option<Span>, base: i64) {
    if let (Some(first), Some(args)) = (layout.first(shifted(span, base)), arguments) {
        child(layout, shifted(args, base), 1, first);
    }
}

fn signature(
    layout: &mut Layout,
    span: Span,
    params: &FormalParameters,
    base: i64,
    return_type: Option<Span>,
    constructor: bool,
) {
    let span = shifted(span, base);
    let Some(first) = layout.first(span) else {
        return;
    };
    let ps = shifted(params.span, base);
    let Some(left) = layout.first(ps) else {
        return;
    };
    let Some(right) = layout.last(ps) else {
        return;
    };
    if left != first || constructor {
        layout.set(left, 1, first);
    }
    let mut elements: Vec<_> = params.items.iter().map(|p| shifted(p.span, base)).collect();
    if let Some(rest) = &params.rest {
        elements.push(shifted(rest.span, base));
    }
    layout.list(
        &elements,
        layout.tokens[left].span,
        Some(layout.tokens[right].span),
        1,
        false,
    );
    if let Some(ret) = return_type {
        if let Some(end) = layout.first(shifted(ret, base)) {
            range(layout, right + 1, end, 1, first);
        }
    }
}

fn property(layout: &mut Layout, span: Span, key: Span, computed: bool, value: Option<Span>) {
    let (Some(first), Some((kf, kl))) = (layout.first(span), layout.first_last(key, 0)) else {
        return;
    };
    let last_key;
    if computed {
        let (Some(left), Some(right)) = (layout.before(kf), layout.after(kl)) else {
            return;
        };
        range(layout, first, left, 0, first);
        layout.list(
            &[key],
            layout.tokens[left].span,
            Some(layout.tokens[right].span),
            1,
            false,
        );
        last_key = right;
    } else {
        range(layout, first, kf, 0, first);
        last_key = kl;
    }
    if let Some(value) = value {
        if let Some(init) = layout.first(value) {
            range(layout, last_key + 1, init, 1, last_key);
        }
    }
}

pub(super) fn apply_node(
    kind: AstKind<'_>,
    parent: Option<AstKind<'_>>,
    base: i64,
    layout: &mut Layout<'_>,
) -> bool {
    let span = shifted(kind.span(), base);
    let Some(first) = layout.first(span) else {
        return true;
    };
    match kind {
        AstKind::TSTypeAnnotation(_) => {
            let anchor = parent
                .and_then(|p| {
                    let span = match p {
                        AstKind::FormalParameter(n) => n.pattern.span(),
                        _ => p.span(),
                    };
                    layout.first(shifted(span, base))
                })
                .unwrap_or(first);
            layout.set(first, 1, anchor);
            if let Some(second) = layout.after(first) {
                layout.set(second, 1, anchor);
            }
            if let Some(before) = layout.before(first) {
                if layout.text(before) == "?" {
                    layout.set(before, 1, anchor);
                }
            }
        }
        AstKind::TSAsExpression(n) => binary_type(
            layout,
            shifted(n.expression.span(), base),
            shifted(n.type_annotation.span(), base),
        ),
        AstKind::TSSatisfiesExpression(n) => binary_type(
            layout,
            shifted(n.expression.span(), base),
            shifted(n.type_annotation.span(), base),
        ),
        AstKind::TSTypeReference(n) => generic(
            layout,
            n.span,
            n.type_arguments.as_ref().map(|a| a.span),
            base,
        ),
        AstKind::TSInstantiationExpression(n) => {
            generic(layout, n.span, Some(n.type_arguments.span), base)
        }
        AstKind::TSClassImplements(n) => generic(
            layout,
            n.span,
            n.type_arguments.as_ref().map(|a| a.span),
            base,
        ),
        AstKind::TSInterfaceHeritage(n) => generic(
            layout,
            n.span,
            n.type_arguments.as_ref().map(|a| a.span),
            base,
        ),
        AstKind::TSTypeParameterInstantiation(n) => {
            list(layout, n.params.iter().map(GetSpan::span), n.span, base)
        }
        AstKind::TSTypeParameterDeclaration(n) => {
            list(layout, n.params.iter().map(GetSpan::span), n.span, base)
        }
        AstKind::TSTypeAliasDeclaration(n) => {
            if let Some(id) = layout.first(shifted(n.id.span, base)) {
                layout.set(id, 1, first);
                if let Some(args) = &n.type_parameters {
                    child(layout, shifted(args.span, base), 1, id);
                }
                let end = n.type_parameters.as_ref().map_or(n.id.span, |a| a.span);
                if let Some(eq) = layout
                    .last(shifted(end, base))
                    .and_then(|i| layout.after(i))
                {
                    layout.set(eq, 1, id);
                }
                child(layout, shifted(n.type_annotation.span(), base), 1, id);
            }
        }
        AstKind::TSFunctionType(n) => signature(
            layout,
            n.span,
            &n.params,
            base,
            Some(n.return_type.span),
            false,
        ),
        AstKind::TSConstructorType(n) => signature(
            layout,
            n.span,
            &n.params,
            base,
            Some(n.return_type.span),
            true,
        ),
        AstKind::TSCallSignatureDeclaration(n) => signature(
            layout,
            n.span,
            &n.params,
            base,
            n.return_type.as_ref().map(|a| a.span),
            false,
        ),
        AstKind::TSConstructSignatureDeclaration(n) => signature(
            layout,
            n.span,
            &n.params,
            base,
            n.return_type.as_ref().map(|a| a.span),
            true,
        ),
        AstKind::TSMethodSignature(n) => {
            signature(
                layout,
                n.span,
                &n.params,
                base,
                n.return_type.as_ref().map(|a| a.span),
                false,
            );
            if let (Some(key_last), Some(left)) = (
                layout.last(shifted(n.key.span(), base)),
                layout.first(shifted(n.params.span, base)),
            ) {
                range(layout, key_last + 1, left, 1, first);
            }
        }
        AstKind::TSTypeLiteral(n) => {
            list(layout, n.members.iter().map(GetSpan::span), n.span, base)
        }
        AstKind::TSInterfaceBody(n) => list(layout, n.body.iter().map(GetSpan::span), n.span, base),
        AstKind::TSModuleBlock(n) => list(
            layout,
            n.directives
                .iter()
                .map(GetSpan::span)
                .chain(n.body.iter().map(GetSpan::span)),
            n.span,
            base,
        ),
        AstKind::TSEnumBody(n) => list(layout, n.members.iter().map(GetSpan::span), n.span, base),
        AstKind::TSPropertySignature(n) => {
            let key = shifted(n.key.span(), base);
            if n.computed {
                property(layout, span, key, true, None);
            }
            if let Some(ty) = &n.type_annotation {
                if let (Some(last), Some(end)) =
                    (layout.last(key), layout.first(shifted(ty.span, base)))
                {
                    range(layout, last + 1, end, 1, first);
                }
            } else if n.optional {
                suffix(layout, span, 1, 1);
            }
        }
        AstKind::TSIndexSignature(n) => {
            let ts = tokens(layout, span);
            if let (Some(&left), Some(&right)) = (
                ts.iter().find(|&&i| layout.text(i) == "["),
                ts.iter().find(|&&i| layout.text(i) == "]"),
            ) {
                let ps: Vec<_> = n.parameters.iter().map(|p| shifted(p.span, base)).collect();
                layout.list(
                    &ps,
                    layout.tokens[left].span,
                    Some(layout.tokens[right].span),
                    1,
                    false,
                );
                if let Some(end) = layout.first(shifted(n.type_annotation.span, base)) {
                    range(layout, right + 1, end, 1, left);
                }
            }
        }
        AstKind::TSArrayType(_) => suffix(layout, span, 2, 0),
        AstKind::TSTupleType(n) => list(
            layout,
            n.element_types.iter().map(GetSpan::span),
            n.span,
            base,
        ),
        AstKind::TSQualifiedName(n) => {
            if let Some(right) = layout.first(shifted(n.right.span, base)) {
                if let Some(dot) = layout.before(right) {
                    layout.set(dot, 1, first);
                }
                layout.set(right, 1, first);
            }
        }
        AstKind::TSImportTypeQualifiedName(n) => {
            if let Some(right) = layout.first(shifted(n.right.span, base)) {
                if let Some(dot) = layout.before(right) {
                    layout.set(dot, 1, first);
                }
                layout.set(right, 1, first);
            }
        }
        AstKind::TSIndexedAccessType(n) => {
            let index = shifted(n.index_type.span(), base);
            if let Some((a, b)) = layout.first_last(index, 0) {
                if let (Some(left), Some(right)) = (layout.before(a), layout.after(b)) {
                    layout.set(left, 1, first);
                    layout.list(
                        &[index],
                        layout.tokens[left].span,
                        Some(layout.tokens[right].span),
                        1,
                        false,
                    );
                }
            }
        }
        AstKind::TSUnionType(n) => union(layout, n.types.iter().map(GetSpan::span), span, base),
        AstKind::TSIntersectionType(n) => {
            union(layout, n.types.iter().map(GetSpan::span), span, base)
        }
        AstKind::TSMappedType(n) => {
            let key = shifted(n.key.span, base);
            if let Some(k) = layout.first(key) {
                if let Some(left) = layout.before(k) {
                    range(layout, first + 1, left, 1, first);
                    let mut xs = vec![key, shifted(n.constraint.span(), base)];
                    if let Some(name) = &n.name_type {
                        xs.push(shifted(name.span(), base));
                    }
                    if let Some(last) = xs
                        .last()
                        .and_then(|s| layout.last(*s))
                        .and_then(|i| layout.after(i))
                    {
                        layout.list(
                            &xs,
                            layout.tokens[left].span,
                            Some(layout.tokens[last].span),
                            1,
                            false,
                        );
                        if let Some(c) = layout.first(shifted(n.constraint.span(), base)) {
                            range(layout, k + 1, c, 1, k);
                        }
                        if let Some(ty) = &n.type_annotation {
                            if let Some(t) = layout.first(shifted(ty.span(), base)) {
                                range(layout, last + 1, t, 1, first);
                            }
                        } else if let Some(end) = layout.last(span) {
                            if end > last + 1 {
                                range(layout, last + 1, end - 1, 1, first);
                            }
                        }
                    }
                }
            }
            if let Some(end) = layout.last(span) {
                layout.set(end, 0, first);
            }
        }
        AstKind::TSTypeParameter(n) => {
            let mut ts = tokens(layout, span);
            for child in [&n.constraint, &n.default].into_iter().flatten() {
                let child = shifted(child.span(), base);
                let cf = layout.first(child);
                ts.retain(|&i| {
                    layout.tokens[i].span.start < child.start
                        || layout.tokens[i].span.end > child.end
                        || Some(i) == cf
                });
            }
            if ts.len() > 1 {
                let second = ts[1];
                layout.set(second, 1, first);
                if layout.text(second) == "extends" {
                    let mut anchor = second;
                    let mut previous = second;
                    for &i in &ts[2..] {
                        if layout.text(i) == "=" {
                            anchor = previous;
                        }
                        layout.set(i, 1, anchor);
                        previous = i;
                    }
                } else {
                    for &i in &ts[2..] {
                        layout.set(i, 1, first);
                    }
                }
            }
        }
        AstKind::TSConditionalType(n) => {
            if let Some(ext) = layout
                .last(shifted(n.check_type.span(), base))
                .and_then(|i| layout.after(i))
            {
                layout.set(ext, 1, first);
                child(layout, shifted(n.extends_type.span(), base), 1, ext);
            }
            let question = layout
                .last(shifted(n.extends_type.span(), base))
                .and_then(|i| layout.after(i));
            let colon = layout
                .last(shifted(n.true_type.span(), base))
                .and_then(|i| layout.after(i));
            for op in [question, colon].into_iter().flatten() {
                layout.set(op, 1, first);
            }
            if let Some(AstKind::TSConditionalType(p)) = parent {
                if p.false_type.span() == n.span {
                    if let Some(pq) = layout
                        .last(shifted(p.extends_type.span(), base))
                        .and_then(|i| layout.after(i))
                    {
                        for op in [question, colon].into_iter().flatten() {
                            layout.copy(op, pq);
                        }
                    }
                }
            }
            if let Some(q) = question {
                child(layout, shifted(n.true_type.span(), base), 1, q);
            }
            if let Some(c) = colon {
                child(layout, shifted(n.false_type.span(), base), 1, c);
            }
        }
        AstKind::TSInterfaceDeclaration(n) => {
            child(layout, shifted(n.id.span, base), 1, first);
            if let Some(id) = layout.first(shifted(n.id.span, base)) {
                if let Some(tp) = &n.type_parameters {
                    child(layout, shifted(tp.span, base), 1, id);
                }
            }
            if let Some(ext) = n
                .extends
                .first()
                .and_then(|n| layout.first(shifted(n.span, base)))
                .and_then(|i| layout.before(i))
            {
                layout.set(ext, 1, first);
                let spans: Vec<_> = n.extends.iter().map(|n| shifted(n.span, base)).collect();
                layout.list(&spans, layout.tokens[ext].span, None, 1, false);
            }
            child(layout, shifted(n.body.span, base), 0, first);
        }
        AstKind::TSEnumDeclaration(n) => declaration(
            layout,
            span,
            shifted(n.id.span, base),
            Some(shifted(n.body.span, base)),
        ),
        AstKind::TSModuleDeclaration(n) => declaration(
            layout,
            span,
            shifted(n.id.span(), base),
            n.body.as_ref().map(|b| shifted(b.span(), base)),
        ),
        AstKind::TSGlobalDeclaration(n) => child(layout, shifted(n.body.span, base), 0, first),
        AstKind::TSEnumMember(n) => property(
            layout,
            span,
            shifted(n.id.span(), base),
            matches!(
                n.id,
                TSEnumMemberName::ComputedString(_) | TSEnumMemberName::ComputedTemplateString(_)
            ),
            n.initializer.as_ref().map(|v| shifted(v.span(), base)),
        ),
        AstKind::TSTypeOperator(_)
        | AstKind::TSTypeQuery(_)
        | AstKind::TSInferType(_)
        | AstKind::TSRestType(_) => {
            if let Some(next) = layout.after(first) {
                layout.set(next, 1, first);
            }
        }
        AstKind::TSTypePredicate(n) => {
            if let Some(op) = layout
                .last(shifted(n.parameter_name.span(), base))
                .and_then(|i| layout.after(i))
            {
                layout.set(op, 1, first);
            }
            if let Some(ty) = &n.type_annotation {
                child(layout, shifted(ty.type_annotation.span(), base), 1, first);
            }
        }
        AstKind::TSOptionalType(_)
        | AstKind::TSNonNullExpression(_)
        | AstKind::JSDocNonNullableType(_) => suffix(layout, span, 1, 1),
        AstKind::TSParenthesizedType(n) => list(layout, [n.type_annotation.span()], n.span, base),
        AstKind::TSTypeAssertion(n) => {
            if let Some(e) = layout.first(shifted(n.expression.span(), base)) {
                if let Some(right) = layout.before(e) {
                    layout.list(
                        &[shifted(n.type_annotation.span(), base)],
                        layout.tokens[first].span,
                        Some(layout.tokens[right].span),
                        1,
                        false,
                    );
                }
                layout.set(e, 1, first);
            }
        }
        AstKind::TSImportType(n) => {
            if let Some(left) = layout.after(first) {
                layout.set(left, 1, first);
                let mut args = vec![shifted(n.source.span, base)];
                if let Some(options) = &n.options {
                    args.push(shifted(options.span, base));
                }
                if let Some(right) = args
                    .last()
                    .and_then(|s| layout.last(*s))
                    .and_then(|i| layout.after(i))
                {
                    layout.list(
                        &args,
                        layout.tokens[left].span,
                        Some(layout.tokens[right].span),
                        1,
                        false,
                    );
                }
            }
            if let Some(q) = &n.qualifier {
                if let Some(qf) = layout.first(shifted(q.span(), base)) {
                    if let Some(dot) = layout.before(qf) {
                        layout.set(dot, 1, first);
                    }
                    layout.set(qf, 1, first);
                }
            }
            if let Some(args) = &n.type_arguments {
                child(layout, shifted(args.span, base), 1, first);
            }
        }
        AstKind::TSImportEqualsDeclaration(n) => {
            if let Some(id) = layout.first(shifted(n.id.span, base)) {
                layout.set(id, 1, first);
                if let Some(eq) = layout.after(id) {
                    layout.set(eq, 1, id);
                }
                child(layout, shifted(n.module_reference.span(), base), 1, id);
            }
        }
        AstKind::TSExternalModuleReference(n) => {
            if let (Some(left), Some(right)) = (layout.after(first), layout.last(span)) {
                layout.set(left, 1, first);
                layout.list(
                    &[shifted(n.expression.span, base)],
                    layout.tokens[left].span,
                    Some(layout.tokens[right].span),
                    1,
                    false,
                );
            }
        }
        AstKind::TSExportAssignment(n) => {
            if let Some(e) = layout.first(shifted(n.expression.span(), base)) {
                if let Some(eq) = layout.before(e) {
                    layout.set(eq, 1, first);
                }
                layout.set(e, 1, first);
            }
        }
        AstKind::TSNamedTupleMember(n) => {
            if let Some(end) = layout.first(shifted(n.element_type.span(), base)) {
                range(layout, first + 1, end, 1, first);
            }
        }
        AstKind::TSNamespaceExportDeclaration(n) => {
            if let Some(end) = layout.first(shifted(n.id.span, base)) {
                range(layout, first + 1, end, 1, first);
            }
        }
        AstKind::TSTemplateLiteralType(n) => {
            for q in n.quasis.iter().skip(1) {
                child(layout, shifted(q.span, base), 0, first);
            }
            for t in &n.types {
                child(layout, shifted(t.span(), base), 1, first);
            }
        }
        AstKind::Decorator(n) => {
            if let Some(second) = layout.after(first) {
                layout.set(second, 0, first);
            }
            decorators(layout, parent, n.span, base, first);
        }
        AstKind::AccessorProperty(n) => property(
            layout,
            span,
            shifted(n.key.span(), base),
            n.computed,
            n.value.as_ref().map(|v| shifted(v.span(), base)),
        ),
        AstKind::StaticBlock(n) => {
            if let Some(left) = tokens(layout, span)
                .into_iter()
                .find(|&i| layout.text(i) == "{")
            {
                range(layout, first + 1, left, 0, first);
                let ps: Vec<_> = n.body.iter().map(|s| shifted(s.span(), base)).collect();
                if let Some(last) = layout.last(span) {
                    layout.list(
                        &ps,
                        layout.tokens[left].span,
                        Some(layout.tokens[last].span),
                        1,
                        false,
                    );
                }
            }
        }
        AstKind::ImportAttribute(n) => property(
            layout,
            span,
            shifted(n.key.span(), base),
            false,
            Some(shifted(n.value.span, base)),
        ),
        AstKind::Class(n) => {
            if let Some(tp) = &n.type_parameters {
                let anchor =
                    n.id.as_ref()
                        .and_then(|id| layout.first(shifted(id.span, base)))
                        .unwrap_or(first);
                child(layout, shifted(tp.span, base), 1, anchor);
            }
            if let (Some(tp), Some(superclass)) = (&n.super_type_arguments, &n.super_class) {
                if let Some(anchor) = layout.first(shifted(superclass.span(), base)) {
                    child(layout, shifted(tp.span, base), 1, anchor);
                }
            }
            if let Some(imp) = n
                .implements
                .first()
                .and_then(|i| layout.first(shifted(i.span, base)))
                .and_then(|i| layout.before(i))
            {
                layout.set(imp, 1, first);
                let spans: Vec<_> = n.implements.iter().map(|i| shifted(i.span, base)).collect();
                layout.list(&spans, layout.tokens[imp].span, None, 1, false);
            }
        }
        AstKind::FormalParameter(n) => {
            if let Some(binding) = layout.first(shifted(n.pattern.span(), base)) {
                if n.accessibility.is_some() || n.readonly || n.r#override {
                    let start = n
                        .decorators
                        .last()
                        .and_then(|d| layout.last(shifted(d.span, base)))
                        .and_then(|i| layout.after(i))
                        .unwrap_or(first);
                    if binding > start {
                        range(layout, start + 1, binding, 1, start);
                    }
                }
                if let Some(init) = &n.initializer {
                    if let Some(init) = layout.first(shifted(init.span(), base)) {
                        if let Some(eq) = layout.before(init) {
                            layout.set(eq, 1, binding);
                        }
                        layout.set(init, 1, binding);
                    }
                }
            }
        }
        AstKind::TSThisParameter(_)
        | AstKind::TSIndexSignatureName(_)
        | AstKind::TSLiteralType(_)
        | AstKind::TSAnyKeyword(_)
        | AstKind::TSStringKeyword(_)
        | AstKind::TSBooleanKeyword(_)
        | AstKind::TSNumberKeyword(_)
        | AstKind::TSNeverKeyword(_)
        | AstKind::TSIntrinsicKeyword(_)
        | AstKind::TSUnknownKeyword(_)
        | AstKind::TSNullKeyword(_)
        | AstKind::TSUndefinedKeyword(_)
        | AstKind::TSVoidKeyword(_)
        | AstKind::TSSymbolKeyword(_)
        | AstKind::TSThisType(_)
        | AstKind::TSObjectKeyword(_)
        | AstKind::TSBigIntKeyword(_) => {}
        _ => return false,
    }
    semicolon(layout, span, first);
    true
}

fn union(layout: &mut Layout, types: impl IntoIterator<Item = Span>, span: Span, base: i64) {
    let Some(first) = layout.first(span) else {
        return;
    };
    let mut spans: Vec<_> = types.into_iter().map(|s| shifted(s, base)).collect();
    if spans.first().and_then(|s| layout.first(*s)) == Some(first) {
        spans.remove(0);
    }
    let level = if layout.is_beginning(layout.tokens[first].span) {
        0
    } else {
        1
    };
    layout.list(&spans, layout.tokens[first].span, None, level, false);
}

fn declaration(layout: &mut Layout, span: Span, id: Span, body: Option<Span>) {
    if let (Some(first), Some(ident)) = (layout.first(span), layout.first(id)) {
        if ident > first + 1 {
            range(layout, first + 1, ident - 1, 0, first);
        }
        layout.set(ident, 1, first);
        if let Some(body) = body {
            if let Some(b) = layout.first(body) {
                layout.set(b, if layout.text(b) == "{" { 0 } else { 1 }, first);
            }
        }
    }
}

fn semicolon(layout: &mut Layout, span: Span, first: usize) {
    if let Some(last) = layout.last(span) {
        if last != first
            && layout.text(last) == ";"
            && layout.after(last).is_none_or(|n| {
                layout.line_of(layout.tokens[last].span.start)
                    < layout.line_of(layout.tokens[n].span.start)
            })
        {
            layout.set(last, 0, first);
        }
    }
}

fn decorators(
    layout: &mut Layout,
    parent: Option<AstKind<'_>>,
    span: Span,
    base: i64,
    first: usize,
) {
    let Some(parent) = parent else {
        return;
    };
    let ds: &[Decorator<'_>] = match parent {
        AstKind::Class(n) => &n.decorators,
        AstKind::MethodDefinition(n) => &n.decorators,
        AstKind::PropertyDefinition(n) => &n.decorators,
        AstKind::AccessorProperty(n) => &n.decorators,
        AstKind::FormalParameter(n) => &n.decorators,
        _ => return,
    };
    if let Some(d) = ds.first() {
        if d.span == span {
            if parent.span().start == span.start {
                if let Some(after) = ds
                    .last()
                    .and_then(|d| layout.last(shifted(d.span, base)))
                    .and_then(|i| layout.after(i))
                {
                    layout.set(after, 0, first);
                }
            } else if let Some(p) = layout.first(shifted(parent.span(), base)) {
                layout.copy(first, p);
            }
        } else if let Some(anchor) = layout.first(shifted(d.span, base)) {
            layout.set(first, 0, anchor);
        }
    }
}
