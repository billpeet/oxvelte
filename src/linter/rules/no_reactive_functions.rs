//! `svelte/no-reactive-functions` — disallow assigning functions to reactive declarations.
//! ⭐ Recommended 💡

use crate::linter::{Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::{Expression, Statement};
use oxc::span::Span;

pub struct NoReactiveFunctions;

impl Rule for NoReactiveFunctions {
    fn name(&self) -> &'static str {
        "svelte/no-reactive-functions"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        // Vendor's `meta.conditions` excludes Svelte 5 runes mode — `$:` has
        // different semantics there.
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
        let script_source = ctx
            .ast
            .instance
            .as_ref()
            .map_or(ctx.source, |script| script.content.as_str());
        let tokens = super::script_token_spans(script_source);

        for stmt in &semantic.nodes().program().body {
            let Statement::LabeledStatement(ls) = stmt else {
                continue;
            };
            if ls.label.name != "$" {
                continue;
            }
            // `$: name = <function expression>`
            let Statement::ExpressionStatement(es) = &ls.body else {
                continue;
            };
            let Expression::AssignmentExpression(ae) = &es.expression else {
                continue;
            };
            let is_fn = matches!(
                &ae.right,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            );
            if !is_fn {
                continue;
            }
            // Vendor reports on the whole `SvelteReactiveStatement`, i.e. the
            // entire `$: name = (...) => {...};`.
            let s = content_offset + ls.span.start;
            let e = content_offset + ls.span.end;
            let mut suggestions = Vec::new();
            let label_tokens: Vec<_> = tokens
                .iter()
                .filter(|token| token.start >= ls.span.start && token.end <= ls.span.end)
                .take(3)
                .collect();
            if let [label, colon, next] = label_tokens.as_slice() {
                let gap = &script_source[colon.end as usize..next.start as usize];
                suggestions.push(Suggestion {
                    description: "Move the function out of the reactive statement".into(),
                    fix: Fix {
                        span: Span::new(content_offset + label.start, content_offset + colon.end),
                        replacement: if gap.chars().any(char::is_whitespace) {
                            "const"
                        } else {
                            "const "
                        }
                        .into(),
                    },
                });
            }
            ctx.diagnostic_with_suggestions(
                "Do not create functions inside reactive statements unless absolutely necessary.",
                Span::new(s, e),
                suggestions,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn function_suggestion_uses_label_tokens_and_preserves_comments() {
        let source = "<!-- 😀 --><script data-note=\">\">$ /* : */ :fn = () => 1;</script>";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty());
        let diagnostics = crate::linter::Linter::all().lint(&parsed.ast, source);
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.rule_name == "svelte/no-reactive-functions")
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
            "<!-- 😀 --><script data-note=\">\">const fn = () => 1;</script>"
        );
    }
}
