//! `svelte/no-extra-reactive-curlies` — disallow unnecessary curly braces in reactive statements.
//! 💡
//!
//! Detects `$: { single_statement; }` patterns where the braces are unnecessary.

use crate::linter::{Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::Statement;
use oxc::span::Span;

pub struct NoExtraReactiveCurlies;

impl Rule for NoExtraReactiveCurlies {
    fn name(&self) -> &'static str {
        "svelte/no-extra-reactive-curlies"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        // Vendor's `meta.conditions` excludes Svelte 5 runes mode.
        if ctx.is_runes {
            return;
        }
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

        for stmt in &semantic.nodes().program().body {
            let Statement::LabeledStatement(ls) = stmt else {
                continue;
            };
            if ls.label.name != "$" {
                continue;
            }
            let Statement::BlockStatement(b) = &ls.body else {
                continue;
            };
            // Flag only when the block contains a single statement — the braces
            // are unnecessary wrapper in that case.
            if b.body.len() != 1 {
                continue;
            }
            let s = content_offset + b.span.start;
            let e = content_offset + b.span.end;
            // Upstream removes the braces and whitespace up to the first and
            // after the last inner token, including comments as tokens.
            let inner = &ctx.source[(s + 1) as usize..(e - 1) as usize];
            ctx.diagnostic_with_suggestions(
                "Do not wrap reactive statements in curly braces unless necessary.",
                Span::new(s, e),
                vec![Suggestion {
                    description: "Remove the unnecessary curly braces.".into(),
                    fix: Fix {
                        span: Span::new(s, e),
                        replacement: inner.trim().to_string(),
                    },
                }],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn brace_removal_retains_inner_comments_and_literal_braces() {
        let source = "<!-- 😀 --><script data-note=\">\">$: { /* before */ value = '}'; /* after */ }</script>";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty());
        let diagnostics = crate::linter::Linter::all().lint(&parsed.ast, source);
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.rule_name == "svelte/no-extra-reactive-curlies")
            .unwrap();
        assert!(diagnostic.fix.is_none());
        let fix = &diagnostic.suggestions[0].fix;
        let mut output = source.to_string();
        output.replace_range(
            fix.span.start as usize..fix.span.end as usize,
            &fix.replacement,
        );
        assert_eq!(
            output,
            "<!-- 😀 --><script data-note=\">\">$: /* before */ value = '}'; /* after */</script>"
        );
    }
}
