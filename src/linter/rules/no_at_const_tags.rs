//! `svelte/no-at-const-tags` migrates reactive legacy const tags to declaration tags.

use crate::ast::TemplateNode;
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use oxc::allocator::Allocator;
use oxc::ast::ast::{Expression, Statement};
use oxc::span::{GetSpan, SourceType, Span};

pub struct NoAtConstTags;

impl Rule for NoAtConstTags {
    fn name(&self) -> &'static str {
        "svelte/no-at-const-tags"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if !ctx.is_runes
            || ctx.runes_explicit_false
            || !ctx.svelte_version.includes_major(5)
            || !ctx.svelte_version.guarantees_minimum_release((5, 56, 0))
        {
            return;
        }
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::ConstTag(tag) = node else {
                return;
            };
            let raw = &ctx.source[tag.span.start as usize..tag.span.end as usize];
            let body = raw[1..].trim_start();
            if !body.starts_with("@const") {
                return;
            }
            let at_offset = tag.span.start + 1 + (raw[1..].len() - body.len()) as u32;
            let allocator = Allocator::default();
            let declaration = format!("const {}", tag.declaration);
            let parsed =
                oxc::parser::Parser::new(&allocator, &declaration, SourceType::ts()).parse();
            let message = "Use `{const ...}` declaration tag instead of legacy `{@const ...}`.";
            if !parsed.errors.is_empty() {
                ctx.diagnostic(message, tag.span);
                return;
            }
            let initializer = parsed.program.body.first().and_then(|statement| {
                let Statement::VariableDeclaration(declaration) = statement else {
                    return None;
                };
                declaration.declarations.first()?.init.as_ref()
            });
            let mut fix = Fix {
                span: Span::new(at_offset, at_offset + 1),
                replacement: String::new(),
            };
            if let Some(initializer) = initializer {
                let already_derived = matches!(initializer.without_parentheses(), Expression::CallExpression(call)
                    if matches!(&call.callee, Expression::Identifier(id) if id.name == "$derived"));
                if !already_derived {
                    let span = initializer.span();
                    let start = tag.declaration_span.start + span.start - 6;
                    let end = tag.declaration_span.start + span.end - 6;
                    fix = Fix {
                        span: Span::new(at_offset, end),
                        replacement: format!(
                            "{}$derived({})",
                            &ctx.source[(at_offset + 1) as usize..start as usize],
                            &ctx.source[start as usize..end as usize]
                        ),
                    };
                }
            }
            ctx.diagnostic_with_fix(message, tag.span, fix);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::NoAtConstTags;
    use crate::{
        ast::TemplateNode,
        linter::{svelte_version_info_from_package_json, LintContext, Rule},
        parser,
    };
    use oxc::allocator::Allocator;

    fn migrate(
        source: &str,
        version: &str,
        runes: bool,
        explicit_false: bool,
    ) -> Vec<crate::linter::LintDiagnostic> {
        let alloc = Allocator::default();
        let parsed = parser::parse_for_lint(source, &alloc);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let mut ctx = LintContext::new(&parsed.ast, source);
        ctx.is_runes = runes;
        ctx.runes_explicit_false = explicit_false;
        ctx.svelte_version = svelte_version_info_from_package_json(
            &serde_json::json!({"dependencies":{"svelte":version}}).to_string(),
        );
        NoAtConstTags.run(&mut ctx);
        ctx.diagnostics
    }

    #[test]
    fn migration_preserves_trivia_and_reparses_as_a_modern_declaration() {
        let source = "<!-- 😀 --><svelte:options runes={true} />{#if yes}{ \t@const /* name */ result = /* before */ ({ text: '😀}', value: count }) /* after */ }{/if}";
        let diagnostics = migrate(source, "5.56.0", true, false);
        assert_eq!(diagnostics.len(), 1);
        let fix = diagnostics[0].fix.as_ref().unwrap();
        let mut output = source.to_string();
        output.replace_range(
            fix.span.start as usize..fix.span.end as usize,
            &fix.replacement,
        );
        assert_eq!(output, "<!-- 😀 --><svelte:options runes={true} />{#if yes}{ \tconst /* name */ result = /* before */ $derived(({ text: '😀}', value: count })) /* after */ }{/if}");
        let alloc = Allocator::default();
        let parsed = parser::parse_for_lint(&output, &alloc);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let block = parsed
            .ast
            .html
            .nodes
            .iter()
            .find_map(|node| {
                if let TemplateNode::IfBlock(block) = node {
                    Some(block)
                } else {
                    None
                }
            })
            .unwrap();
        let TemplateNode::ConstTag(tag) = &block.consequent.nodes[0] else {
            panic!("declaration node")
        };
        assert!(tag.declaration.contains("$derived("));
        assert_eq!(
            &output[tag.declaration_span.start as usize..tag.declaration_span.end as usize],
            tag.declaration
        );
        assert!(migrate(&output, "5.56.0", true, false).is_empty());
    }

    #[test]
    fn already_derived_only_removes_the_legacy_marker() {
        let source = "{#if yes}{@const value = $derived(count + 1)}{/if}";
        let diagnostics = migrate(source, "^5.56.0", true, false);
        let fix = diagnostics[0].fix.as_ref().unwrap();
        assert_eq!(&source[fix.span.start as usize..fix.span.end as usize], "@");
        assert!(fix.replacement.is_empty());
    }

    #[test]
    fn never_offers_new_syntax_to_older_or_legacy_projects() {
        let source = "{#if yes}{@const value = count + 1}{/if}";
        for (version, runes, explicit_false) in [
            ("5.49.2", true, false),
            ("^5.49.2", true, false),
            ("4.2.0", true, false),
            ("5.56.0", false, false),
            ("5.56.0", true, true),
            ("*", true, false),
        ] {
            assert!(
                migrate(source, version, runes, explicit_false).is_empty(),
                "{version}"
            );
        }
    }
}
