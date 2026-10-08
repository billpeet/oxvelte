//! `svelte/no-not-function-handler` — disallow non-function event handlers.
//! ⭐ Recommended

use crate::ast::{Attribute, AttributeValue, AttributeValuePart, DirectiveKind, TemplateNode};
use crate::linter::{walk_template_nodes, LintContext, Rule};
use oxc::ast::ast::{Expression, VariableDeclarationKind};
use oxc::ast::AstKind;
use oxc::span::{GetSpan, SourceType, Span};

pub struct NoNotFunctionHandler;

impl Rule for NoNotFunctionHandler {
    fn name(&self) -> &'static str {
        "svelte/no-not-function-handler"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let mut findings: Vec<(String, Span)> = Vec::new();
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            for (idx, attr) in el.attributes.iter().enumerate() {
                match attr {
                    // `on:event={…}` directive — typed AST is in attribute_meta[idx].
                    Attribute::Directive {
                        kind: DirectiveKind::EventHandler,
                        ..
                    } => {
                        let Some(expr) = el.attribute_expression_ast(idx) else {
                            continue;
                        };
                        check_handler(
                            expr,
                            ctx,
                            &mut findings,
                            el.attribute_meta[idx]
                                .expression_span
                                .unwrap_or_else(|| attr_value_span(attr)),
                        );
                    }
                    // `onclick={…}` Svelte-5 / HTML on-attribute. Restricted to
                    // names matching the curated `is_event_name` test.
                    Attribute::NormalAttribute { name, value, .. } if is_event_name(name) => {
                        match value {
                            AttributeValue::Expression(_) => {
                                let Some(expr) = el.attribute_expression_ast(idx) else {
                                    continue;
                                };
                                check_handler(
                                    expr,
                                    ctx,
                                    &mut findings,
                                    el.attribute_meta[idx]
                                        .expression_span
                                        .unwrap_or_else(|| attr_value_span(attr)),
                                );
                            }
                            AttributeValue::Concat(parts) => {
                                for (part_idx, part) in parts.iter().enumerate() {
                                    if !matches!(part, AttributeValuePart::Expression(_)) {
                                        continue;
                                    }
                                    let Some(expr) =
                                        el.attribute_part_expression_ast(idx, part_idx)
                                    else {
                                        continue;
                                    };
                                    let span = el
                                        .attribute_meta
                                        .get(idx)
                                        .and_then(|m| m.parts.get(part_idx))
                                        .and_then(|p| p.expression_span)
                                        .unwrap_or_else(|| attr_value_span(attr));
                                    check_handler(expr, ctx, &mut findings, span);
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        });
        for (msg, span) in findings {
            ctx.diagnostic(msg, span);
        }
    }
}

const EVENT_BASE_NAMES: &[&str] = &[
    "abort",
    "animationend",
    "animationiteration",
    "animationstart",
    "auxclick",
    "beforeinput",
    "beforematch",
    "beforetoggle",
    "blur",
    "cancel",
    "canplay",
    "canplaythrough",
    "change",
    "click",
    "close",
    "compositionend",
    "compositionstart",
    "compositionupdate",
    "contentvisibilityautostatechange",
    "contextmenu",
    "copy",
    "cuechange",
    "cut",
    "dblclick",
    "drag",
    "dragend",
    "dragenter",
    "dragexit",
    "dragleave",
    "dragover",
    "dragstart",
    "drop",
    "durationchange",
    "emptied",
    "encrypted",
    "ended",
    "error",
    "focus",
    "focusin",
    "focusout",
    "formdata",
    "fullscreenchange",
    "fullscreenerror",
    "gamepadconnected",
    "gamepaddisconnected",
    "gotpointer",
    "gotpointercapture",
    "input",
    "introend",
    "introstart",
    "invalid",
    "keydown",
    "keypress",
    "keyup",
    "load",
    "loadeddata",
    "loadedmetadata",
    "loadstart",
    "lostpointer",
    "lostpointercapture",
    "message",
    "messageerror",
    "mousedown",
    "mouseenter",
    "mouseleave",
    "mousemove",
    "mouseout",
    "mouseover",
    "mouseup",
    "outroend",
    "outrostart",
    "paste",
    "pause",
    "play",
    "playing",
    "pointercancel",
    "pointerdown",
    "pointerenter",
    "pointerleave",
    "pointermove",
    "pointerout",
    "pointerover",
    "pointerup",
    "progress",
    "ratechange",
    "reset",
    "resize",
    "scroll",
    "scrollend",
    "seeked",
    "seeking",
    "select",
    "selectionchange",
    "selectstart",
    "stalled",
    "submit",
    "suspend",
    "timeupdate",
    "toggle",
    "touchcancel",
    "touchend",
    "touchmove",
    "touchstart",
    "transitioncancel",
    "transitionend",
    "transitionrun",
    "transitionstart",
    "visibilitychange",
    "volumechange",
    "waiting",
    "wheel",
];

/// True for Svelte 5 event-handler attributes from vendor's `EVENT_NAMES`
/// table: `on${event}` and `on${event}capture`.
fn is_event_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("on") else {
        return false;
    };
    let base = rest.strip_suffix("capture").unwrap_or(rest);
    EVENT_BASE_NAMES.contains(&base)
}

fn attr_value_span(attr: &Attribute) -> Span {
    match attr {
        Attribute::NormalAttribute { span, .. } | Attribute::Directive { span, .. } => *span,
        Attribute::Spread { span } => *span,
    }
}

fn check_handler<'a>(
    expr: &'a Expression<'a>,
    ctx: &LintContext<'a>,
    findings: &mut Vec<(String, Span)>,
    span: Span,
) {
    let raw = &ctx.source[span.start as usize..span.end as usize];
    let offset = span.start + (raw.len() - raw.trim_start().len()) as u32;
    let local = expr.span();
    let span = Span::new(
        offset + local.start.saturating_sub(6),
        offset + local.end.saturating_sub(6),
    );
    let resolved = resolve_handler_expression(expr, ctx, span);
    if let Some(phrase) = non_function_phrase(resolved) {
        findings.push((format!("Unexpected {} in event handler.", phrase), span));
    }
}

/// Vendor's PHRASES table. `null` literal returns `None` (vendor's
/// `node.value == null` short-circuit), so `onclick={null}` is silently
/// allowed. `NewExpression` is *not* in the table — `new Decorator()` may
/// well return a function.
fn non_function_phrase(expr: &Expression<'_>) -> Option<&'static str> {
    match expr {
        Expression::ArrayExpression(_) => Some("array"),
        Expression::ObjectExpression(_) => Some("object"),
        Expression::ClassExpression(_) => Some("class"),
        Expression::StringLiteral(_) => Some("string value"),
        Expression::TemplateLiteral(_) => Some("string value"),
        Expression::BooleanLiteral(_) => Some("boolean value"),
        Expression::NumericLiteral(_) => Some("number value"),
        Expression::BigIntLiteral(_) => Some("bigint value"),
        Expression::RegExpLiteral(_) => Some("regex value"),
        _ => None,
    }
}

/// Follow const aliases using their actual script symbol. Template locals are
/// conservatively left unresolved because they belong to a different scope.
pub(super) fn resolve_handler_expression<'a>(
    expr: &'a Expression<'a>,
    ctx: &LintContext<'a>,
    span: Span,
) -> &'a Expression<'a> {
    let Expression::Identifier(id) = expr else {
        return expr;
    };
    if template_shadows(&ctx.ast.html.nodes, span, id.name.as_str()) {
        return expr;
    }
    let Some(sem) = ctx
        .instance_semantic
        .filter(|sem| {
            sem.scoping()
                .find_binding(sem.scoping().root_scope_id(), id.name)
                .is_some()
        })
        .or(ctx.module_semantic)
    else {
        return expr;
    };
    let Some(mut sid) = sem
        .scoping()
        .find_binding(sem.scoping().root_scope_id(), id.name)
    else {
        return expr;
    };
    let mut seen = Vec::new();
    let mut current = expr;
    loop {
        if seen.contains(&sid) {
            return current;
        }
        seen.push(sid);
        let decl = sem.scoping().symbol_declaration(sid);
        let Some(vd) = std::iter::once(decl)
            .chain(sem.nodes().ancestor_ids(decl))
            .find_map(|node| match sem.nodes().kind(node) {
                AstKind::VariableDeclarator(vd) => Some(vd),
                _ => None,
            })
        else {
            return current;
        };
        if !std::iter::once(decl).chain(sem.nodes().ancestor_ids(decl)).any(|node| matches!(sem.nodes().kind(node), AstKind::VariableDeclaration(d) if d.kind == VariableDeclarationKind::Const)) { return current; }
        let Some(init) = vd.init.as_ref() else {
            return current;
        };
        current = init;
        let Expression::Identifier(id) = init else {
            return init;
        };
        let Some(next) = sem.scoping().get_reference(id.reference_id()).symbol_id() else {
            return init;
        };
        sid = next;
    }
}

fn binds(pattern: &str, name: &str) -> bool {
    let allocator = oxc::allocator::Allocator::default();
    let source = format!("({pattern}) => {{}}");
    let parsed = oxc::parser::Parser::new(&allocator, &source, SourceType::ts()).parse();
    let semantic = oxc::semantic::SemanticBuilder::new()
        .build(&parsed.program)
        .semantic;
    let found = semantic
        .nodes()
        .iter()
        .any(|node| matches!(node.kind(), AstKind::BindingIdentifier(id) if id.name == name));
    found
}

fn template_shadows(nodes: &[TemplateNode<'_>], span: Span, name: &str) -> bool {
    let contains = |nodes: &[TemplateNode<'_>]| {
        nodes
            .iter()
            .any(|node| node_span(node).start <= span.start && span.end <= node_span(node).end)
    };
    if !contains(nodes) {
        return false;
    }
    for node in nodes {
        if let TemplateNode::ConstTag(tag) = node {
            if tag
                .declaration
                .split_once('=')
                .is_some_and(|(pattern, _)| binds(pattern, name))
            {
                return true;
            }
        }
    }
    nodes.iter().any(|node| match node {
        TemplateNode::Element(el) if contains(&el.children) => {
            el.attributes.iter().any(|attr| match attr {
                Attribute::Directive {
                    kind: DirectiveKind::Let,
                    name: local,
                    value,
                    ..
                } => binds(
                    match value {
                        AttributeValue::Expression(pattern) => pattern,
                        _ => local,
                    },
                    name,
                ),
                _ => false,
            }) || template_shadows(&el.children, span, name)
        }
        TemplateNode::EachBlock(block) => {
            (contains(&block.body.nodes)
                && (binds(&block.context, name) || block.index.as_deref() == Some(name)))
                || template_shadows(&block.body.nodes, span, name)
                || block
                    .fallback
                    .as_ref()
                    .is_some_and(|f| template_shadows(&f.nodes, span, name))
        }
        TemplateNode::AwaitBlock(block) => [
            (block.pending.as_ref(), None),
            (block.then.as_ref(), block.then_binding.as_deref()),
            (block.catch.as_ref(), block.catch_binding.as_deref()),
        ]
        .iter()
        .any(|(fragment, binding)| {
            fragment.is_some_and(|f| {
                contains(&f.nodes)
                    && (binding.is_some_and(|p| binds(p, name))
                        || template_shadows(&f.nodes, span, name))
            })
        }),
        TemplateNode::SnippetBlock(block) => {
            contains(&block.body.nodes)
                && (binds(&block.params, name) || template_shadows(&block.body.nodes, span, name))
        }
        TemplateNode::IfBlock(block) => {
            template_shadows(&block.consequent.nodes, span, name)
                || block.alternate.as_ref().is_some_and(|node| {
                    template_shadows(std::slice::from_ref(node.as_ref()), span, name)
                })
        }
        TemplateNode::KeyBlock(block) => template_shadows(&block.body.nodes, span, name),
        _ => false,
    })
}

fn node_span(node: &TemplateNode<'_>) -> Span {
    match node {
        TemplateNode::Text(node) => node.span,
        TemplateNode::Element(node) => node.span,
        TemplateNode::MustacheTag(node) => node.span,
        TemplateNode::RawMustacheTag(node) => node.span,
        TemplateNode::DebugTag(node) => node.span,
        TemplateNode::ConstTag(node) => node.span,
        TemplateNode::RenderTag(node) => node.span,
        TemplateNode::Comment(node) => node.span,
        TemplateNode::IfBlock(node) => node.span,
        TemplateNode::EachBlock(node) => node.span,
        TemplateNode::AwaitBlock(node) => node.span,
        TemplateNode::KeyBlock(node) => node.span,
        TemplateNode::SnippetBlock(node) => node.span,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        linter::{LintDiagnostic, Linter},
        parser,
    };
    use oxc::allocator::Allocator;
    fn lint(source: &str) -> Vec<LintDiagnostic> {
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-not-function-handler")
            .collect()
    }
    #[test]
    fn diagnostics_point_at_the_original_expression_with_unicode_and_quotes() {
        let source = "<!-- 😀 --><script data-note=\">\">const original = []; const alias = original;</script><button on:click={ alias } onclick=\"prefix{1}{'x'}\"/>";
        let diagnostics = lint(source);
        let values: Vec<_> = diagnostics
            .iter()
            .map(|d| &source[d.span.start as usize..d.span.end as usize])
            .collect();
        assert_eq!(values, ["alias", "1", "'x'"]);
    }
    #[test]
    fn aliases_resolve_symbols_and_cycles_stop_without_false_findings() {
        assert_eq!(lint("<script>const value = []; const handler = value;</script><button on:click={handler}/>").len(), 1);
        assert!(lint("<script>const first = second; const second = first;</script><button on:click={first}/>").is_empty());
        assert!(lint(
            "<script>const value = []; let handler = value;</script><button on:click={handler}/>"
        )
        .is_empty());
        assert_eq!(
            lint("<script module>const handler = [];</script><button on:click={handler}/>").len(),
            1
        );
    }
    #[test]
    fn template_bindings_shadow_script_constants_in_their_own_branches() {
        let source = "<script>const handler = [];</script>{#each items as { handler }}<button on:click={handler}/>{:else}<button on:click={handler}/>{/each}{#await promise then handler}<button on:click={handler}/>{/await}{#snippet row(handler)}<button on:click={handler}/>{/snippet}<Component let:handler><button on:click={handler}/></Component>";
        assert_eq!(lint(source).len(), 1);
    }
}
