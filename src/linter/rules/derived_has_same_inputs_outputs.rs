//! `svelte/derived-has-same-inputs-outputs` names callback parameters after stores.

use crate::linter::{Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::{
    BindingIdentifier, BindingPattern, Expression, ImportDeclarationSpecifier, ModuleExportName,
    Statement,
};
use oxc::ast::AstKind;
use oxc::span::{GetSpan, Span};

pub struct DerivedHasSameInputsOutputs;

impl Rule for DerivedHasSameInputsOutputs {
    fn name(&self) -> &'static str {
        "svelte/derived-has-same-inputs-outputs"
    }
    fn applies_to_scripts(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let Some(semantic) = ctx.instance_semantic else {
            return;
        };
        let offset = ctx
            .ast
            .instance
            .as_ref()
            .map_or(ctx.instance_content_offset, |script| {
                script.content_span.start
            });
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
                        if name == "derived" {
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
                Expression::StaticMemberExpression(member) if member.property.name == "derived" => {
                    matches!(&member.object, Expression::Identifier(id) if scoping.get_reference(id.reference_id()).symbol_id().is_some_and(|sid| namespaces.contains(&sid)))
                }
                _ => false,
            };
            if !imported {
                continue;
            }
            let Some(input) = call.arguments.first().and_then(|arg| arg.as_expression()) else {
                continue;
            };
            let Some(callback) = call.arguments.get(1).and_then(|arg| arg.as_expression()) else {
                continue;
            };
            let (parameter, callback_span) = match callback {
                Expression::ArrowFunctionExpression(function) => {
                    (function.params.items.first(), function.span)
                }
                Expression::FunctionExpression(function) => {
                    (function.params.items.first(), function.span)
                }
                _ => continue,
            };
            let Some(parameter) = parameter else { continue };
            let mut pairs: Vec<(&str, &BindingIdentifier<'_>)> = Vec::new();
            match (input, &parameter.pattern) {
                (Expression::Identifier(store), BindingPattern::BindingIdentifier(binding)) => {
                    pairs.push((store.name.as_str(), binding))
                }
                (Expression::ArrayExpression(stores), BindingPattern::ArrayPattern(bindings)) => {
                    for (store, binding) in stores.elements.iter().zip(&bindings.elements) {
                        if let (
                            Some(Expression::Identifier(store)),
                            Some(BindingPattern::BindingIdentifier(binding)),
                        ) = (store.as_expression(), binding.as_ref())
                        {
                            pairs.push((store.name.as_str(), binding));
                        }
                    }
                }
                _ => continue,
            }
            for (store, binding) in pairs {
                let expected = format!("${store}");
                if binding.name == expected.as_str() {
                    continue;
                }
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
                let mut suggestions = Vec::new();
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
                        replacement
                            .push_str(&ctx.source[cursor as usize..(offset + span.start) as usize]);
                        replacement.push_str(&text);
                        cursor = offset + span.end;
                    }
                    suggestions.push(Suggestion {
                        description: format!(
                            "Rename the parameter from {} to {}.",
                            binding.name, expected
                        ),
                        fix: Fix {
                            span: Span::new(start, end),
                            replacement,
                        },
                    });
                }
                ctx.diagnostic_with_suggestions(
                    format!("The argument name should be '{}'.", expected),
                    Span::new(offset + binding.span.start, offset + binding.span.end),
                    suggestions,
                );
            }
        }
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
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|diagnostic| diagnostic.rule_name == "svelte/derived-has-same-inputs-outputs")
            .collect()
    }

    #[test]
    fn rename_preserves_shadowed_references_and_script_prefix() {
        let source = "<!-- 😀 --><script data-note=\">\">import { derived as make } from 'svelte/store'; make(a, (value) => { value; (() => { const value = 1; value; })(); });</script>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "value"
        );
        let fix = &diagnostics[0].suggestions[0].fix;
        let mut output = source.to_string();
        output.replace_range(
            fix.span.start as usize..fix.span.end as usize,
            &fix.replacement,
        );
        assert_eq!(output, "<!-- 😀 --><script data-note=\">\">import { derived as make } from 'svelte/store'; make(a, ($a) => { $a; (() => { const value = 1; value; })(); });</script>");
    }

    #[test]
    fn array_parameters_offer_independent_edits_and_capture_checks() {
        let source = "<script>import * as stores from 'svelte/store'; stores.derived([a, b], ([x, y]) => { x; y; }); stores.derived(a, (x) => { (() => { const $a = 1; x; })(); }); stores.derived(a, (a) => a);</script>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 4);
        assert_eq!(diagnostics[0].suggestions.len(), 1);
        assert_eq!(diagnostics[1].suggestions.len(), 1);
        assert_eq!(diagnostics[2].suggestions.len(), 0);
        assert_eq!(diagnostics[3].suggestions.len(), 1);
        let fix = &diagnostics[0].suggestions[0].fix;
        let mut output = source.to_string();
        output.replace_range(
            fix.span.start as usize..fix.span.end as usize,
            &fix.replacement,
        );
        assert!(output.contains("([$a, y]) => { $a; y; }"));
    }

    #[test]
    fn local_and_wrong_module_derived_functions_are_ignored() {
        let source = "<script>import { derived } from 'other'; derived(a, (x) => x); function local(derived) { derived(a, (x) => x); }</script>";
        assert!(lint(source).is_empty());
    }

    #[test]
    fn independent_inner_replacement_name_does_not_block_rename() {
        let source = "<script>import { derived } from 'svelte/store'; derived(a, (value) => { value; (() => { const $a = 1; $a; })(); });</script>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].suggestions.len(), 1);
    }

    #[test]
    fn rename_preserves_object_and_assignment_shorthand_keys() {
        let source = "<script>import { derived } from 'svelte/store'; derived(a, (x) => { ({x} = obj); ({x = 1} = obj); return {x}; });</script>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1);
        let fix = &diagnostics[0].suggestions[0].fix;
        let mut output = source.to_string();
        output.replace_range(
            fix.span.start as usize..fix.span.end as usize,
            &fix.replacement,
        );
        assert_eq!(output, "<script>import { derived } from 'svelte/store'; derived(a, ($a) => { ({x: $a} = obj); ({x: $a = 1} = obj); return {x: $a}; });</script>");
    }

    #[test]
    fn default_parameter_references_are_checked_for_capture() {
        let source = "<script>import { derived } from 'svelte/store'; derived(a, (x, y = $a) => x); derived(a, function(x, y = $a) { return x; });</script>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics
            .iter()
            .all(|diagnostic| diagnostic.suggestions.is_empty()));
    }
}
