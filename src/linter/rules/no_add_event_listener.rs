//! `svelte/no-add-event-listener` — disallow `addEventListener` in Svelte components.
//! 💡

use crate::linter::{Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::Expression;
use oxc::ast::AstKind;
use oxc::span::{GetSpan, Span};

pub struct NoAddEventListener;

impl Rule for NoAddEventListener {
    fn name(&self) -> &'static str {
        "svelte/no-add-event-listener"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        // Vendor condition: this rule only runs for Svelte 5 projects. Keep
        // unknown versions enabled so standalone fixtures still exercise it.
        if !ctx.svelte_version.is_unknown() && !ctx.svelte_version.includes_major(5) {
            return;
        }
        if let Some(semantic) = ctx.instance_semantic {
            let offset = ctx
                .ast
                .instance
                .as_ref()
                .map_or(ctx.instance_content_offset, |script| {
                    script.content_span.start
                });
            check_semantic(ctx, semantic, offset);
        }
        if let Some(semantic) = ctx.module_semantic {
            let offset = ctx
                .ast
                .module
                .as_ref()
                .map_or(ctx.module_content_offset, |script| {
                    script.content_span.start
                });
            check_semantic(ctx, semantic, offset);
        }
    }
}

fn check_semantic(
    ctx: &mut LintContext<'_>,
    semantic: &oxc::semantic::Semantic<'_>,
    content_offset: u32,
) {
    let source = ctx.source;
    let script_source = &source
        [content_offset as usize..(content_offset + semantic.nodes().program().span.end) as usize];
    let tokens = super::script_token_spans(script_source);
    for node in semantic.nodes().iter() {
        let AstKind::CallExpression(ce) = node.kind() else {
            continue;
        };
        let target = match &ce.callee {
            // Bare call: `addEventListener('msg', handler)`
            Expression::Identifier(id) if id.name == "addEventListener" => "window".to_string(),
            // Member call: `window.addEventListener(...)`, `foo.bar.addEventListener(...)`
            Expression::StaticMemberExpression(mem) if mem.property.name == "addEventListener" => {
                let span = mem.object.without_parentheses().span();
                script_source[span.start as usize..span.end as usize].to_string()
            }
            Expression::ComputedMemberExpression(mem) if matches!(&mem.expression, Expression::Identifier(id) if id.name == "addEventListener") =>
            {
                let span = mem.object.without_parentheses().span();
                script_source[span.start as usize..span.end as usize].to_string()
            }
            _ => continue,
        };
        let callee_span = ce.callee.span();
        let mut suggestions = Vec::new();
        if let Some(next) = tokens
            .iter()
            .find(|token| token.start >= callee_span.end && token.end <= ce.span.end)
        {
            suggestions.push(Suggestion {
                description: "Use `on` from `svelte/events` instead".into(),
                fix: Fix {
                    span: Span::new(
                        content_offset + callee_span.start,
                        content_offset + next.end,
                    ),
                    replacement: format!(
                        "on{}{target}, ",
                        &script_source[callee_span.end as usize..next.end as usize]
                    ),
                },
            });
        }
        ctx.diagnostic_with_suggestions(
            "Do not use `addEventListener`. Use the `on` function from `svelte/events` instead.",
            Span::new(content_offset + ce.span.start, content_offset + ce.span.end),
            suggestions,
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn on_suggestions_preserve_comments_and_exact_receiver_source() {
        let source = "<!-- 😀 --><script data-note=\">\">deep.target.addEventListener /* ( */ ('event', callback); addEventListener('event', callback);</script><script module data-note=\">\">node[addEventListener]('event', callback);</script>";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty());
        let diagnostics: Vec<_> = crate::linter::Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-add-event-listener")
            .collect();
        assert_eq!(diagnostics.len(), 3);
        let expected = ["on /* ( */ (deep.target, ", "on(window, ", "on(node, "];
        for (diagnostic, replacement) in diagnostics.iter().zip(expected) {
            assert!(diagnostic.fix.is_none());
            assert_eq!(diagnostic.suggestions[0].fix.replacement, replacement);
            assert!(
                source[diagnostic.span.start as usize..diagnostic.span.end as usize]
                    .ends_with("callback)")
            );
        }
    }
}
