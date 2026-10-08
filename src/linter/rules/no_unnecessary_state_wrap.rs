//! `svelte/no-unnecessary-state-wrap` — disallow wrapping values that are already reactive with `$state`.
//! ⭐ Recommended 💡
//!
//! Svelte's reactive classes (SvelteSet, SvelteMap, etc.) are already reactive
//! and don't need `$state()` wrapping.

use crate::ast::{Attribute, AttributeValue, DirectiveKind, Fragment, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::{
    Argument, Expression, ImportDeclarationSpecifier, ModuleExportName, Statement,
};
use oxc::ast::AstKind;
use oxc::semantic::SymbolId;
use oxc::span::{GetSpan, Span};

const REACTIVE_CLASSES: &[&str] = &[
    "SvelteSet",
    "SvelteMap",
    "SvelteURL",
    "SvelteURLSearchParams",
    "SvelteDate",
    "MediaQuery",
];

pub struct NoUnnecessaryStateWrap;

impl Rule for NoUnnecessaryStateWrap {
    fn name(&self) -> &'static str {
        "svelte/no-unnecessary-state-wrap"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let Some(semantic) = ctx.instance_semantic else {
            return;
        };
        let content_offset = ctx
            .ast
            .instance
            .as_ref()
            .map_or(ctx.instance_content_offset, |script| {
                script.content_span.start
            });

        let opts = ctx
            .config
            .options
            .as_ref()
            .and_then(|o| o.as_array())
            .and_then(|a| a.first());
        let additional: Vec<String> = opts
            .and_then(|o| o.get("additionalReactiveClasses"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let allow_reassign = opts
            .and_then(|o| o.get("allowReassign"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Match reactive constructors by imported symbol, preserving shadows.
        let mut imports = Vec::new();
        let mut namespaces = Vec::new();
        // Track named and namespace imports from the reactive classes module.
        let nodes = semantic.nodes();
        let program = nodes.program();
        for stmt in &program.body {
            let Statement::ImportDeclaration(imp) = stmt else {
                continue;
            };
            let src = imp.source.value.as_str();
            let is_svelte = src == "svelte/reactivity";
            let Some(specifiers) = &imp.specifiers else {
                continue;
            };
            for spec in specifiers {
                if let ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) = spec {
                    if is_svelte {
                        namespaces.push(s.local.symbol_id());
                    }
                }
                let ImportDeclarationSpecifier::ImportSpecifier(s) = spec else {
                    continue;
                };
                let imported = match &s.imported {
                    ModuleExportName::IdentifierName(n) => n.name.as_str(),
                    ModuleExportName::IdentifierReference(n) => n.name.as_str(),
                    ModuleExportName::StringLiteral(l) => l.value.as_str(),
                };
                if is_svelte && REACTIVE_CLASSES.contains(&imported) {
                    imports.push((s.local.symbol_id(), imported.to_string()));
                }
            }
        }

        let scoping = semantic.scoping();
        for node in nodes.iter() {
            let AstKind::CallExpression(ce) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &ce.callee else {
                continue;
            };
            if callee.name != "$state" {
                continue;
            }
            // Wrapping must be the direct initializer, rather than nested in
            // another expression that happens to belong to a declaration.
            let AstKind::VariableDeclarator(decl) = nodes.parent_kind(node.id()) else {
                continue;
            };
            let oxc::ast::ast::BindingPattern::BindingIdentifier(binding) = &decl.id else {
                continue;
            };
            let Some(Expression::CallExpression(init)) = &decl.init else {
                continue;
            };
            if init.span != ce.span {
                continue;
            }
            let decl_symbol = binding.symbol_id();
            if allow_reassign
                && is_symbol_reassigned(decl_symbol, binding.span, scoping, nodes, &ctx.ast.html)
            {
                continue;
            }
            for arg in &ce.arguments {
                let (constructor, target_span) = match arg {
                    Argument::NewExpression(expr) => (&expr.callee, expr.span),
                    Argument::CallExpression(expr) => (&expr.callee, expr.span),
                    _ => continue,
                };
                let original = match constructor {
                    Expression::Identifier(id) => {
                        if additional.iter().any(|name| name == id.name.as_str()) {
                            Some(id.name.to_string())
                        } else {
                            let symbol = scoping.get_reference(id.reference_id()).symbol_id();
                            imports
                                .iter()
                                .find(|(sid, _)| Some(*sid) == symbol)
                                .map(|(_, name)| name.clone())
                        }
                    }
                    Expression::StaticMemberExpression(member) => {
                        if let Expression::Identifier(id) = &member.object {
                            let symbol = scoping.get_reference(id.reference_id()).symbol_id();
                            if symbol.is_some_and(|sid| namespaces.contains(&sid))
                                && REACTIVE_CLASSES.contains(&member.property.name.as_str())
                            {
                                Some(member.property.name.to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                let Some(original) = original else { continue };
                ctx.diagnostic_with_suggestions(
                    format!(
                        "{} is already reactive, $state wrapping is unnecessary.",
                        original
                    ),
                    Span::new(
                        content_offset + target_span.start,
                        content_offset + target_span.end,
                    ),
                    vec![Suggestion {
                        description: "Remove unnecessary $state wrapping".into(),
                        fix: Fix {
                            span: Span::new(
                                content_offset + ce.span.start,
                                content_offset + ce.span.end,
                            ),
                            replacement: ctx.source[(content_offset + target_span.start) as usize
                                ..(content_offset + target_span.end) as usize]
                                .to_string(),
                        },
                    }],
                );
            }
        }
    }
}

/// Has this symbol been reassigned anywhere (JS write reference, or a template
/// `bind:` directive that would write through to this name)?
fn is_symbol_reassigned<'a>(
    sid: SymbolId,
    declaration_span: Span,
    scoping: &'a oxc::semantic::Scoping,
    nodes: &'a oxc::semantic::AstNodes<'a>,
    html: &'a Fragment,
) -> bool {
    if scoping
        .get_resolved_references(sid)
        .any(|r| r.is_write() && nodes.kind(r.node_id()).span() != declaration_span)
    {
        return true;
    }
    let name = scoping.symbol_name(sid);
    let mut found = false;
    walk_template_nodes(html, &mut |node| {
        if found {
            return;
        }
        let TemplateNode::Element(el) = node else {
            return;
        };
        for attr in &el.attributes {
            let Attribute::Directive {
                kind: DirectiveKind::Binding,
                name: dir_name,
                value,
                ..
            } = attr
            else {
                continue;
            };
            // `bind:name` shorthand — the directive name is the bound symbol.
            if dir_name == name {
                found = true;
                return;
            }
            // `bind:anything={name}` / nested member writing through name.
            if let AttributeValue::Expression(text) = value {
                let trimmed = text.trim();
                let base = trimmed
                    .split(|c: char| c == '.' || c == '[')
                    .next()
                    .unwrap_or(trimmed);
                if base == name {
                    found = true;
                    return;
                }
            }
        }
    });
    found
}

#[cfg(test)]
mod tests {
    use crate::{
        linter::{LintDiagnostic, Linter, RuleConfig},
        parser,
    };
    use oxc::allocator::Allocator;

    fn lint(source: &str, options: serde_json::Value) -> Vec<LintDiagnostic> {
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint_with_config(
                &parsed.ast,
                source,
                RuleConfig {
                    options: Some(options),
                    ..RuleConfig::default()
                },
            )
            .into_iter()
            .filter(|diagnostic| diagnostic.rule_name == "svelte/no-unnecessary-state-wrap")
            .collect()
    }

    #[test]
    fn import_identity_and_suggestion_ranges_survive_script_attributes() {
        let source = "<!-- 😀 --><script data-note=\">\">import { SvelteMap as Map } from 'svelte/reactivity'; import * as reactive from 'svelte/reactivity'; const map = $state(new Map()); const set = $state(reactive.SvelteSet()); function shadow(Map) { const local = $state(new Map()); } </script>";
        let diagnostics = lint(source, serde_json::json!([]));
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "new Map()"
        );
        let fix = &diagnostics[0].suggestions[0].fix;
        assert_eq!(
            &source[fix.span.start as usize..fix.span.end as usize],
            "$state(new Map())"
        );
        assert_eq!(fix.replacement, "new Map()");
        assert_eq!(
            diagnostics[1].suggestions[0].fix.replacement,
            "reactive.SvelteSet()"
        );
        assert!(lint("<script>import { SvelteSet } from 'other'; const value = $state(new SvelteSet());</script>", serde_json::json!([])).is_empty());
    }

    #[test]
    fn allow_reassign_uses_the_declared_symbol_in_nested_scopes() {
        let source = "<script>import { SvelteSet } from 'svelte/reactivity'; let value = $state(new SvelteSet()); function nested() { let value = $state(new SvelteSet()); value = new SvelteSet(); }</script>";
        let diagnostics = lint(source, serde_json::json!([{"allowReassign": true}]));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].span.start < source.find("function nested").unwrap() as u32);
    }

    #[test]
    fn custom_factories_require_a_direct_binding_initializer() {
        let source = "<script>const valid = $state(Custom()); const nested = keep($state(Custom()));</script>";
        let diagnostics = lint(
            source,
            serde_json::json!([{"additionalReactiveClasses": ["Custom"]}]),
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].suggestions[0].fix.replacement, "Custom()");
    }
}
