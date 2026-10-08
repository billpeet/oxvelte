//! `svelte/require-store-callbacks-use-set-param` names callback parameters after stores.

use crate::linter::{Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::{
    BindingPattern, Expression, ImportDeclarationSpecifier, ModuleExportName, Statement,
};
use oxc::ast::AstKind;
use oxc::span::{GetSpan, Span};

pub struct RequireStoreCallbacksUseSetParam;

impl Rule for RequireStoreCallbacksUseSetParam {
    fn name(&self) -> &'static str {
        "svelte/require-store-callbacks-use-set-param"
    }
    fn applies_to_scripts(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        for (semantic, offset) in [
            ctx.instance_semantic.map(|s| {
                (
                    s,
                    ctx.ast
                        .instance
                        .as_ref()
                        .map_or(ctx.instance_content_offset, |script| {
                            script.content_span.start
                        }),
                )
            }),
            ctx.module_semantic.map(|s| {
                (
                    s,
                    ctx.ast
                        .module
                        .as_ref()
                        .map_or(ctx.module_content_offset, |script| {
                            script.content_span.start
                        }),
                )
            }),
        ]
        .into_iter()
        .flatten()
        {
            let nodes = semantic.nodes();
            let scoping = semantic.scoping();
            let mut imports = Vec::new();
            let mut namespaces = Vec::new();
            for statement in &nodes.program().body {
                let Statement::ImportDeclaration(import) = statement else {
                    continue;
                };
                if import.source.value != "svelte/store" {
                    continue;
                }
                for specifier in import.specifiers.iter().flatten() {
                    match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                            let name = match &specifier.imported {
                                ModuleExportName::IdentifierName(id) => id.name.as_str(),
                                ModuleExportName::IdentifierReference(id) => id.name.as_str(),
                                ModuleExportName::StringLiteral(value) => value.value.as_str(),
                            };
                            if matches!(name, "readable" | "writable") {
                                imports.push(specifier.local.symbol_id());
                            }
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                            namespaces.push(specifier.local.symbol_id())
                        }
                        _ => {}
                    }
                }
            }
            for node in nodes.iter() {
                let AstKind::CallExpression(call) = node.kind() else {
                    continue;
                };
                let imported = match &call.callee {
                    Expression::Identifier(id) => scoping
                        .get_reference(id.reference_id())
                        .symbol_id()
                        .is_some_and(|sid| imports.contains(&sid)),
                    Expression::StaticMemberExpression(member)
                        if matches!(member.property.name.as_str(), "readable" | "writable") =>
                    {
                        matches!(&member.object, Expression::Identifier(id) if scoping.get_reference(id.reference_id()).symbol_id().is_some_and(|sid| namespaces.contains(&sid)))
                    }
                    _ => false,
                };
                if !imported {
                    continue;
                }
                let Some(callback) = call.arguments.get(1).and_then(|arg| arg.as_expression())
                else {
                    continue;
                };
                let (params, callback_span, _body_span) = match callback {
                    Expression::ArrowFunctionExpression(function) => {
                        (&function.params, function.span, function.body.span)
                    }
                    Expression::FunctionExpression(function) => {
                        let Some(body) = &function.body else { continue };
                        (&function.params, function.span, body.span)
                    }
                    _ => continue,
                };
                let binding = match params.items.first() {
                    Some(parameter) => match &parameter.pattern {
                        BindingPattern::BindingIdentifier(binding)
                            if binding.name != "set" && parameter.initializer.is_none() =>
                        {
                            Some(binding)
                        }
                        _ => continue,
                    },
                    None if params.rest.is_none() => None,
                    None => continue,
                };
                let expected = String::from("set");
                let mut suggestions = Vec::new();
                if let Some(binding) = binding {
                    let sid = binding.symbol_id();
                    let scope = scoping.symbol_scope_id(sid);
                    let references: Vec<_> = scoping.get_resolved_references(sid).collect();
                    // Avoid both changing which binding a use resolves to and
                    // capturing existing references to the replacement name.
                    let conflict = scoping
                        .get_binding(scope, expected.as_str().into())
                        .is_some()
                        || references.iter().any(|reference| {
                            scoping
                                .find_binding(
                                    nodes.get_node(reference.node_id()).scope_id(),
                                    expected.as_str().into(),
                                )
                                .is_some()
                        })
                        || nodes.iter().any(|other| {
                            let AstKind::IdentifierReference(id) = other.kind() else {
                                return false;
                            };
                            id.name == expected.as_str()
                                && id.span.start >= callback_span.start
                                && id.span.end <= callback_span.end
                                && !scoping
                                    .get_reference(id.reference_id())
                                    .symbol_id()
                                    .is_some_and(|target| {
                                        // Inner bindings keep their own references after
                                        // the parameter is renamed. Outer names would
                                        // instead become captured by the parameter.
                                        let target_scope = scoping.symbol_scope_id(target);
                                        target_scope != scope
                                            && scoping
                                                .scope_ancestors(target_scope)
                                                .any(|ancestor| ancestor == scope)
                                    })
                        });

                    if !conflict {
                        let mut edits = vec![(binding.span, expected.clone())];
                        edits.extend(references.iter().map(|reference| {
                            let node_id = reference.node_id();
                            let span = nodes.kind(node_id).span();
                            // Shorthand syntax uses the same token for the key
                            // and variable. Expand it so renaming preserves keys.
                            let shorthand = match nodes.parent_kind(node_id) {
                                AstKind::ObjectProperty(property) => property.shorthand,
                                AstKind::AssignmentTargetPropertyIdentifier(property) => {
                                    property.binding.span == span
                                }
                                _ => false,
                            };
                            let replacement = if shorthand {
                                format!("{}: {}", binding.name, expected)
                            } else {
                                expected.clone()
                            };
                            (span, replacement)
                        }));
                        edits.sort_by_key(|(span, _)| span.start);
                        edits.dedup_by_key(|(span, _)| *span);
                        let start = offset + edits[0].0.start;
                        let end = offset + edits.last().unwrap().0.end;
                        let mut replacement = String::new();
                        let mut cursor = start;
                        for (span, text) in edits {
                            replacement.push_str(
                                &ctx.source[cursor as usize..(offset + span.start) as usize],
                            );
                            replacement.push_str(&text);
                            cursor = offset + span.end;
                        }
                        suggestions.push(Suggestion {
                            description: format!(
                                "Rename parameter from {} to `set`.",
                                binding.name
                            ),
                            fix: Fix {
                                span: Span::new(start, end),
                                replacement,
                            },
                        });
                    }
                } else {
                    // Adding a parameter must not capture an existing free `set`, or collide
                    // with declarations in the callback's own scope.
                    let conflict = nodes.iter().any(|node| match node.kind() {
                        AstKind::IdentifierReference(id) if id.name == "set" => {
                            id.span.start >= callback_span.start && id.span.end <= callback_span.end
                        }
                        AstKind::BindingIdentifier(id) if id.name == "set" => {
                            id.span.start >= callback_span.start && id.span.end <= callback_span.end
                        }
                        _ => false,
                    });
                    if !conflict {
                        let position = offset + params.span.start;
                        if ctx.source.as_bytes().get(position as usize) == Some(&b'(') {
                            let position = position + 1;
                            suggestions.push(Suggestion {
                                description: "Add a `set` parameter.".into(),
                                fix: Fix {
                                    span: Span::new(position, position),
                                    replacement: "set".into(),
                                },
                            });
                        }
                    }
                }
                ctx.diagnostic_with_suggestions(
                    "Store callbacks must use `set` param.",
                    Span::new(offset + callback_span.start, offset + callback_span.end),
                    suggestions,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    fn lint(source: &str) -> Vec<crate::linter::LintDiagnostic> {
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        crate::linter::Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/require-store-callbacks-use-set-param")
            .collect()
    }
    #[test]
    fn imported_callbacks_rename_symbols_and_preserve_shorthand() {
        let source = "<!-- 😀 --><script data-note=\">\">import { readable as make } from 'svelte/store'; import * as stores from 'svelte/store'; make(0, value => { (() => { let value = 1; value; })(); return {value}; }); stores.writable(0, () => 1); function shadow(make) { make(0, value => value); }</script>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2);
        let fix = &diagnostics[0].suggestions[0].fix;
        let mut output = source.to_string();
        output.replace_range(
            fix.span.start as usize..fix.span.end as usize,
            &fix.replacement,
        );
        assert!(output.contains(
            "make(0, set => { (() => { let value = 1; value; })(); return {value: set}; })"
        ));
        assert_eq!(diagnostics[1].suggestions[0].fix.replacement, "set");
        assert!(
            lint("<script>import { readable } from 'other'; readable(0, () => 1);</script>")
                .is_empty()
        );
    }
    #[test]
    fn parameter_insertion_skips_comments_and_function_names() {
        let source = "<script>import { readable } from 'svelte/store'; readable(0, function /* ( comment */ named /* ( */ () {}); readable(0, async /* ( */ () => 1);</script>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2);
        for diagnostic in diagnostics {
            let fix = &diagnostic.suggestions[0].fix;
            let mut output = source.to_string();
            output.replace_range(
                fix.span.start as usize..fix.span.end as usize,
                &fix.replacement,
            );
            assert!(output.contains("(set)"));
            assert!(output.contains("/* ( comment */"));
        }
    }
    #[test]
    fn callback_conflicts_defaults_rest_and_module_scripts() {
        let source = "<script>import { readable } from 'svelte/store'; readable(0, (value, set) => value); readable(0, (value) => { (() => { let set; value; })(); }); readable(0, () => set); readable(0, () => { let set; }); readable(0, (value = 1) => value); readable(0, (...args) => args); readable(0, (value, other = set) => value);</script><script context=\"module\">import { writable } from 'svelte/store'; writable(0, () => 1);</script>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 6);
        assert!(diagnostics[..5].iter().all(|d| d.suggestions.is_empty()));
        assert_eq!(diagnostics[5].suggestions.len(), 1);
    }
}
