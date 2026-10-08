//! `svelte/no-goto-without-base` — the deprecated base-path check for goto.
use crate::linter::{LintContext, Rule};
use oxc::ast::{ast::Expression, AstKind};
use oxc::semantic::{Semantic, SymbolId};
use oxc::span::{GetSpan, Span};
pub struct NoGotoWithoutBase;
impl Rule for NoGotoWithoutBase {
    fn name(&self) -> &'static str {
        "svelte/no-goto-without-base"
    }
    fn applies_to_scripts(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        for (semantic, offset) in [
            (
                ctx.instance_semantic,
                ctx.ast
                    .instance
                    .as_ref()
                    .map_or(0, |script| script.content_span.start),
            ),
            (
                ctx.module_semantic,
                ctx.ast
                    .module
                    .as_ref()
                    .map_or(0, |script| script.content_span.start),
            ),
        ] {
            let Some(semantic) = semantic else { continue };
            for node in semantic.nodes().iter() {
                let AstKind::CallExpression(call) = node.kind() else {
                    continue;
                };
                if !import_reference(
                    &call.callee,
                    semantic,
                    "$app/navigation",
                    "goto",
                    &mut Vec::new(),
                ) {
                    continue;
                }
                let Some(argument) = call.arguments.first() else {
                    continue;
                };
                let safe = argument.as_expression().is_some_and(|expression| {
                    match expression.get_inner_expression() {
                        Expression::StringLiteral(literal) => absolute_uri(literal.value.as_str()),
                        Expression::BinaryExpression(binary) => {
                            matches!(&binary.left, Expression::Identifier(_))
                                && import_reference(
                                    &binary.left,
                                    semantic,
                                    "$app/paths",
                                    "base",
                                    &mut Vec::new(),
                                )
                        }
                        Expression::TemplateLiteral(template) => {
                            template
                                .quasis
                                .first()
                                .is_some_and(|q| q.value.raw.is_empty())
                                && template.expressions.first().is_some_and(|e| {
                                    matches!(e, Expression::Identifier(_))
                                        && import_reference(
                                            e,
                                            semantic,
                                            "$app/paths",
                                            "base",
                                            &mut Vec::new(),
                                        )
                                })
                        }
                        _ => false,
                    }
                });
                if !safe {
                    let span = argument.span();
                    ctx.diagnostic(
                        "Found a goto() call with a url that isn't prefixed with the base path.",
                        Span::new(offset + span.start, offset + span.end),
                    );
                }
            }
        }
    }
}
fn absolute_uri(value: &str) -> bool {
    value.find(':').is_some_and(|colon| {
        value[..colon]
            .bytes()
            .all(|b| b == b'+' || b.is_ascii_alphabetic())
    })
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
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn argument_spans_use_parsed_script_offsets() {
        for attributes in ["data-note=\">\"", "module data-note=\">\""] {
            let source = format!("<!-- 😀 -->\n<script {attributes}>\nimport {{ goto }} from '$app/navigation';\ngoto('/route');\n</script>");
            let allocator = Allocator::default();
            let parsed = parser::parse(&source, &allocator);
            assert!(parsed.errors.is_empty());
            let diagnostics = Linter::all().lint(&parsed.ast, &source);
            let finding = diagnostics
                .iter()
                .find(|d| d.rule_name == "svelte/no-goto-without-base")
                .unwrap();
            let start = source.find("'/route'").unwrap() as u32;
            assert_eq!(
                finding.span,
                oxc::span::Span::new(start, start + "'/route'".len() as u32)
            );
        }
    }
    #[test]
    fn arguments_and_import_identity_follow_the_legacy_contract() {
        let source = r#"<script>
import { goto as navigate } from '$app/navigation';
import { base } from '$app/paths';
navigate(dynamic);
navigate(...paths);
navigate('custom+scheme:target');
navigate(`${base}/ok`);
navigate(base + '/ok');
function shadow(navigate) { navigate('/ignored'); }
function shadowBase(base) { navigate(base + '/invalid'); }
navigate(`${'/prefix'}${base}`);
</script>"#;
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty());
        let diagnostics = Linter::all().lint(&parsed.ast, source);
        let findings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.rule_name == "svelte/no-goto-without-base")
            .map(|d| &source[d.span.start as usize..d.span.end as usize])
            .collect();
        assert_eq!(
            findings,
            [
                "dynamic",
                "...paths",
                "base + '/invalid'",
                "`${'/prefix'}${base}`"
            ]
        );
    }
}
