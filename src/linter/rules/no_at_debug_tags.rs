//! `svelte/no-at-debug-tags` — disallow the use of `{@debug}`.
//! ⭐ Recommended, 💡 Suggestion

use crate::ast::TemplateNode;
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule, Suggestion};

pub struct NoAtDebugTags;

impl Rule for NoAtDebugTags {
    fn name(&self) -> &'static str {
        "svelte/no-at-debug-tags"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            if let TemplateNode::DebugTag(tag) = node {
                ctx.diagnostic_with_suggestions(
                    "Unexpected `{@debug}`.",
                    tag.span,
                    vec![Suggestion {
                        description: "Remove `{@debug}` from the source".into(),
                        fix: Fix {
                            span: tag.span,
                            replacement: String::new(),
                        },
                    }],
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn debug_removal_preserves_surrounding_unicode_and_text() {
        let source = "😀 before {#if yes}{@debug value} after{/if}";
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty());
        let diagnostics = Linter::all().lint(&parsed.ast, source);
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.rule_name == "svelte/no-at-debug-tags")
            .unwrap();
        assert!(diagnostic.fix.is_none());
        let fix = &diagnostic.suggestions[0].fix;
        let mut output = source.to_string();
        output.replace_range(
            fix.span.start as usize..fix.span.end as usize,
            &fix.replacement,
        );
        assert_eq!(output, "😀 before {#if yes} after{/if}");
    }
}
