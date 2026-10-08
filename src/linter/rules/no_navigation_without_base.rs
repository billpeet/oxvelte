//! `svelte/no-navigation-without-base` — require imported base-path prefixes.
use crate::ast::{Attribute, AttributeValue, AttributeValuePart, Fragment, TemplateNode};
use crate::linter::{LintContext, Rule};
use oxc::ast::{ast::Expression, AstKind};
use oxc::semantic::{Semantic, SymbolId};
use oxc::span::{GetSpan, SourceType, Span};
use std::collections::HashSet;
pub struct NoNavigationWithoutBase;
impl Rule for NoNavigationWithoutBase {
    fn name(&self) -> &'static str {
        "svelte/no-navigation-without-base"
    }
    fn applies_to_scripts(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let options = ctx
            .config
            .options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|v| v.first())
            .cloned();
        let ignored = |key: &str| {
            options
                .as_ref()
                .and_then(|v| v.get(key))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        for (semantic, offset) in [
            (ctx.instance_semantic, ctx.instance_content_offset),
            (ctx.module_semantic, ctx.module_content_offset),
        ] {
            let Some(semantic) = semantic else { continue };
            for node in semantic.nodes().iter() {
                let AstKind::CallExpression(call) = node.kind() else {
                    continue;
                };
                let Some(name) = [
                    ("goto", "ignoreGoto"),
                    ("pushState", "ignorePushState"),
                    ("replaceState", "ignoreReplaceState"),
                ]
                .iter()
                .find_map(|(name, option)| {
                    (!ignored(option)
                        && import_reference(
                            &call.callee,
                            semantic,
                            "$app/navigation",
                            name,
                            &mut Vec::new(),
                        ))
                    .then_some(*name)
                }) else {
                    continue;
                };
                let Some(argument) = call.arguments.first() else {
                    continue;
                };
                let safe = argument.as_expression().is_some_and(|expression| {
                    (name != "goto" && expression_is_empty(expression))
                        || prefixed(
                            expression,
                            semantic,
                            false,
                            &HashSet::new(),
                            &mut Vec::new(),
                        )
                });
                if !safe {
                    let span = argument.span();
                    ctx.diagnostic(format!("Found a {name}() call with a url that isn't prefixed with the base path."), Span::new(offset + span.start, offset + span.end));
                }
            }
        }
        if !ignored("ignoreLinks") {
            check_fragment(&ctx.ast.html, ctx, &HashSet::new());
        }
    }
}
fn expression_is_empty(expression: &Expression<'_>) -> bool {
    match expression.get_inner_expression() {
        Expression::StringLiteral(l) => l.value.is_empty(),
        Expression::TemplateLiteral(t) => {
            t.expressions.is_empty() && t.quasis.len() == 1 && t.quasis[0].value.raw.is_empty()
        }
        _ => false,
    }
}
fn absolute_uri(value: &str) -> bool {
    value.find(':').is_some_and(|colon| {
        value[..colon]
            .bytes()
            .all(|b| b == b'+' || b.is_ascii_alphabetic())
    })
}
fn link_exempt(expression: &Expression<'_>, fragment: bool) -> bool {
    let check = |value: &str| {
        if fragment {
            value.starts_with('#')
        } else {
            absolute_uri(value)
        }
    };
    match expression.get_inner_expression() {
        Expression::StringLiteral(l) => check(l.value.as_str()),
        Expression::BinaryExpression(b) => {
            link_exempt(&b.left, fragment) || (!fragment && link_exempt(&b.right, false))
        }
        Expression::TemplateLiteral(t) if fragment => {
            t.quasis
                .first()
                .is_some_and(|q| check(q.value.raw.as_str()))
                || t.expressions.first().is_some_and(|e| link_exempt(e, true))
        }
        Expression::TemplateLiteral(t) => {
            t.quasis.iter().any(|q| check(q.value.raw.as_str()))
                || t.expressions.iter().any(|e| link_exempt(e, false))
        }
        _ => false,
    }
}
fn prefixed(
    expression: &Expression<'_>,
    semantic: &Semantic<'_>,
    template: bool,
    shadows: &HashSet<String>,
    seen: &mut Vec<SymbolId>,
) -> bool {
    match expression.get_inner_expression() {
        Expression::Identifier(id) => {
            let symbol = if template {
                if shadows.contains(id.name.as_str()) {
                    return false;
                }
                semantic
                    .scoping()
                    .find_binding(semantic.scoping().root_scope_id(), id.name)
            } else {
                semantic
                    .scoping()
                    .get_reference(id.reference_id())
                    .symbol_id()
            };
            let Some(symbol) = symbol else { return false };
            prefix_symbol(symbol, semantic, seen)
        }
        Expression::StaticMemberExpression(member) if member.property.name == "base" => {
            let Expression::Identifier(id) = member.object.get_inner_expression() else {
                return false;
            };
            let symbol = if template {
                if shadows.contains(id.name.as_str()) {
                    return false;
                }
                semantic
                    .scoping()
                    .find_binding(semantic.scoping().root_scope_id(), id.name)
            } else {
                semantic
                    .scoping()
                    .get_reference(id.reference_id())
                    .symbol_id()
            };
            symbol.is_some_and(|symbol| namespace_symbol(symbol, semantic, "$app/paths"))
        }
        Expression::BinaryExpression(binary)
            if binary.operator == oxc::syntax::operator::BinaryOperator::Addition =>
        {
            prefixed(&binary.left, semantic, template, shadows, seen)
        }
        Expression::TemplateLiteral(literal) => {
            literal
                .quasis
                .first()
                .is_some_and(|q| q.value.raw.is_empty())
                && literal
                    .expressions
                    .first()
                    .is_some_and(|e| prefixed(e, semantic, template, shadows, seen))
        }
        _ => false,
    }
}
fn prefix_symbol(symbol: SymbolId, semantic: &Semantic<'_>, seen: &mut Vec<SymbolId>) -> bool {
    if seen.contains(&symbol) {
        return false;
    }
    seen.push(symbol);
    let declaration = semantic.scoping().symbol_declaration(symbol);
    for node in std::iter::once(declaration).chain(semantic.nodes().ancestor_ids(declaration)) {
        match semantic.nodes().kind(node) {
            AstKind::ImportDeclaration(import) => return import.source.value == "$app/paths" && import.specifiers.as_ref().is_some_and(|specifiers| specifiers.iter().any(|specifier| matches!(specifier, oxc::ast::ast::ImportDeclarationSpecifier::ImportSpecifier(s) if s.local.symbol_id.get() == Some(symbol) && s.imported.name() == "base"))),
            AstKind::VariableDeclarator(variable) => return variable.init.as_ref().is_some_and(|init| prefixed(init, semantic, false, &HashSet::new(), seen)),
            _ => {}
        }
    }
    false
}
fn namespace_symbol(symbol: SymbolId, semantic: &Semantic<'_>, source: &str) -> bool {
    let declaration = semantic.scoping().symbol_declaration(symbol);
    std::iter::once(declaration).chain(semantic.nodes().ancestor_ids(declaration)).any(|node| matches!(semantic.nodes().kind(node), AstKind::ImportDeclaration(import) if import.source.value == source && import.specifiers.as_ref().is_some_and(|specifiers| specifiers.iter().any(|s| matches!(s, oxc::ast::ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) if s.local.symbol_id.get() == Some(symbol))))))
}
fn binding_names(pattern: &str) -> HashSet<String> {
    let allocator = oxc::allocator::Allocator::default();
    let source = format!("({pattern}) => {{}}");
    let parsed = oxc::parser::Parser::new(&allocator, &source, SourceType::ts()).parse();
    let semantic = oxc::semantic::SemanticBuilder::new()
        .build(&parsed.program)
        .semantic;
    semantic
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
        })
        .collect()
}
fn check_fragment<'a>(
    fragment: &'a Fragment<'a>,
    ctx: &mut LintContext<'a>,
    inherited: &HashSet<String>,
) {
    check_nodes(&fragment.nodes, ctx, inherited);
}
fn check_nodes<'a>(
    nodes: &'a [TemplateNode<'a>],
    ctx: &mut LintContext<'a>,
    inherited: &HashSet<String>,
) {
    let mut shadows = inherited.clone();
    for node in nodes {
        if let TemplateNode::ConstTag(tag) = node {
            if let Some((pattern, _)) = tag.declaration.split_once('=') {
                shadows.extend(binding_names(pattern));
            }
        }
    }
    for node in nodes {
        match node {
            TemplateNode::Element(element) => {
                if element.name == "a" {
                    for (index, attribute) in element.attributes.iter().enumerate() {
                        let Attribute::NormalAttribute { name, value, .. } = attribute else {
                            continue;
                        };
                        if name != "href" {
                            continue;
                        }
                        let meta = &element.attribute_meta[index];
                        let semantic = ctx.instance_semantic.or(ctx.module_semantic);
                        let safe_expr = |expression| {
                            link_exempt(expression, false)
                                || link_exempt(expression, true)
                                || semantic.is_some_and(|semantic| {
                                    prefixed(expression, semantic, true, &shadows, &mut Vec::new())
                                })
                        };
                        let (safe, span) = match value {
                            AttributeValue::Static(value) => (
                                absolute_uri(value) || value.starts_with('#'),
                                meta.value_span,
                            ),
                            AttributeValue::Expression(_) => (
                                element
                                    .attribute_expression_ast(index)
                                    .is_some_and(safe_expr),
                                meta.mustache_span,
                            ),
                            AttributeValue::Concat(parts) => match parts.first() {
                                Some(AttributeValuePart::Static(value)) => (
                                    absolute_uri(value) || value.starts_with('#'),
                                    meta.parts.first().map(|p| p.span),
                                ),
                                Some(AttributeValuePart::Expression(_)) => (
                                    element
                                        .attribute_part_expression_ast(index, 0)
                                        .is_some_and(safe_expr),
                                    meta.parts.first().map(|p| p.span),
                                ),
                                None => (false, meta.value_span),
                            },
                            AttributeValue::True => continue,
                        };
                        if !safe {
                            if let Some(span) = span {
                                ctx.diagnostic("Found a link with a url that isn't prefixed with the base path.", span);
                            }
                        }
                    }
                }
                let mut children_shadows = shadows.clone();
                for attribute in &element.attributes {
                    if let Attribute::Directive {
                        kind: crate::ast::DirectiveKind::Let,
                        name,
                        value,
                        ..
                    } = attribute
                    {
                        children_shadows.extend(binding_names(match value {
                            AttributeValue::Expression(value) => value,
                            _ => name,
                        }));
                    }
                }
                check_nodes(&element.children, ctx, &children_shadows);
            }
            TemplateNode::IfBlock(block) => {
                check_fragment(&block.consequent, ctx, &shadows);
                if let Some(alternate) = &block.alternate {
                    check_nodes(std::slice::from_ref(alternate.as_ref()), ctx, &shadows);
                }
            }
            TemplateNode::EachBlock(block) => {
                let mut nested = shadows.clone();
                nested.extend(binding_names(&block.context));
                nested.extend(block.index.iter().cloned());
                check_fragment(&block.body, ctx, &nested);
                if let Some(fallback) = &block.fallback {
                    check_fragment(fallback, ctx, &shadows);
                }
            }
            TemplateNode::AwaitBlock(block) => {
                if let Some(pending) = &block.pending {
                    check_fragment(pending, ctx, &shadows);
                }
                for (body, binding) in [
                    (&block.then, &block.then_binding),
                    (&block.catch, &block.catch_binding),
                ] {
                    if let Some(body) = body {
                        let mut nested = shadows.clone();
                        if let Some(binding) = binding {
                            nested.extend(binding_names(binding));
                        }
                        check_fragment(body, ctx, &nested);
                    }
                }
            }
            TemplateNode::KeyBlock(block) => check_fragment(&block.body, ctx, &shadows),
            TemplateNode::SnippetBlock(block) => {
                let mut nested = shadows.clone();
                nested.extend(binding_names(&block.params));
                check_fragment(&block.body, ctx, &nested);
            }
            _ => {}
        }
    }
}
fn import_reference(
    expression: &Expression<'_>,
    semantic: &Semantic<'_>,
    source: &str,
    exported: &str,
    seen: &mut Vec<SymbolId>,
) -> bool {
    match expression.get_inner_expression() {
        Expression::Identifier(id) => {
            let Some(symbol) = semantic
                .scoping()
                .get_reference(id.reference_id())
                .symbol_id()
            else {
                return false;
            };
            if seen.contains(&symbol) {
                return false;
            }
            seen.push(symbol);
            let declaration = semantic.scoping().symbol_declaration(symbol);
            for node in
                std::iter::once(declaration).chain(semantic.nodes().ancestor_ids(declaration))
            {
                match semantic.nodes().kind(node) {
                    AstKind::ImportDeclaration(import) => return import.source.value == source && import.specifiers.as_ref().is_some_and(|specifiers| specifiers.iter().any(|specifier| matches!(specifier, oxc::ast::ast::ImportDeclarationSpecifier::ImportSpecifier(s) if s.local.symbol_id.get() == Some(symbol) && s.imported.name() == exported))),
                    AstKind::VariableDeclarator(variable) => return variable.init.as_ref().is_some_and(|init| import_reference(init, semantic, source, exported, seen)),
                    _ => {}
                }
            }
            false
        }
        Expression::StaticMemberExpression(member) if member.property.name == exported => {
            let Expression::Identifier(id) = member.object.get_inner_expression() else {
                return false;
            };
            let Some(symbol) = semantic
                .scoping()
                .get_reference(id.reference_id())
                .symbol_id()
            else {
                return false;
            };
            let declaration = semantic.scoping().symbol_declaration(symbol);
            std::iter::once(declaration).chain(semantic.nodes().ancestor_ids(declaration)).any(|node| matches!(semantic.nodes().kind(node), AstKind::ImportDeclaration(import) if import.source.value == source && import.specifiers.as_ref().is_some_and(|specifiers| specifiers.iter().any(|s| matches!(s, oxc::ast::ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) if s.local.symbol_id.get() == Some(symbol))))))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        linter::{Linter, RuleConfig},
        parser,
    };
    use oxc::allocator::Allocator;

    fn findings(source: &str, options: serde_json::Value) -> Vec<String> {
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint_with_config(
                &parsed.ast,
                source,
                RuleConfig {
                    options: Some(options),
                    ..Default::default()
                },
            )
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-navigation-without-base")
            .map(|d| source[d.span.start as usize..d.span.end as usize].to_owned())
            .collect()
    }
    #[test]
    fn imported_prefixes_dynamic_arguments_and_scopes() {
        let source = r#"<script>
import { goto as navigate, pushState, replaceState } from '$app/navigation';
import { base } from '$app/paths';
import * as paths from '$app/paths';
const first = base + '/one';
const second = `${first}/two`;
const cycle = cycle;
navigate(second);
navigate(paths.base + '/ok');
navigate(dynamic);
navigate(...args);
navigate('https://example.com');
navigate(cycle);
function shadow(navigate) { navigate('/ignored'); }
function shadowBase(base) { navigate(base + '/invalid'); }
function shadowPaths(paths) { navigate(paths.base + '/invalid'); }
pushState('');
replaceState(``);
pushState(dynamic);
</script>"#;
        assert_eq!(
            findings(source, serde_json::json!([])),
            [
                "dynamic",
                "...args",
                "'https://example.com'",
                "cycle",
                "base + '/invalid'",
                "paths.base + '/invalid'",
                "dynamic"
            ]
        );
        assert!(findings(source, serde_json::json!([{ "ignoreGoto": true, "ignorePushState": true, "ignoreReplaceState": true }])).is_empty());
    }
    #[test]
    fn hrefs_use_value_spans_and_template_binding_scopes() {
        let source = r#"<script>
import { base } from '$app/paths';
const route = base + '/route';
const raw = '/raw';
</script>
<a href="relative">relative</a>
<a href={raw}>dynamic</a>
<a href={route}>prefixed</a>
<a href={'custom+protocol:target'}>external</a>
<a href={'part' + '://external'}>external</a>
<a href={'#' + raw}>fragment</a>
{#each [1] as base}
<a href={base + '/bad'}>shadowed</a>
{:else}
<a href={base + '/ok'}>outer</a>
{/each}
{#if true}
{@const base = '/local'}
<a href={base + '/bad'}>local const</a>
{/if}
<a href={base + '/ok'}>outer</a>"#;
        assert_eq!(
            findings(source, serde_json::json!([])),
            ["relative", "{raw}", "{base + '/bad'}", "{base + '/bad'}"]
        );
        assert!(findings(source, serde_json::json!([{ "ignoreLinks": true }])).is_empty());
    }
    #[test]
    fn unrelated_modules_do_not_provide_navigation_or_base_imports() {
        let source = r#"<script>
import { goto as unrelated } from './other';
import { goto } from '$app/navigation';
import { base } from './other';
unrelated('/ignored');
goto(base + '/bad');
</script>
<a href={base + '/bad'}>wrong import</a>"#;
        assert_eq!(
            findings(source, serde_json::json!([])),
            ["base + '/bad'", "{base + '/bad'}"]
        );
    }
}
