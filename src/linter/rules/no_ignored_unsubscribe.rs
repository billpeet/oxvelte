//! `svelte/no-ignored-unsubscribe` — disallow ignoring store subscribe return value.

use crate::linter::{LintContext, Rule};
use oxc::ast::ast::Expression;
use oxc::ast::AstKind;
use oxc::span::Span;

pub struct NoIgnoredUnsubscribe;

impl Rule for NoIgnoredUnsubscribe {
    fn name(&self) -> &'static str {
        "svelte/no-ignored-unsubscribe"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        for (semantic, content_offset) in [
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

            for node in nodes.iter() {
                let AstKind::CallExpression(ce) = node.kind() else {
                    continue;
                };
                let property_span = match &ce.callee {
                    Expression::StaticMemberExpression(mem) if mem.property.name == "subscribe" => {
                        mem.property.span
                    }
                    Expression::ComputedMemberExpression(mem) => match &mem.expression {
                        Expression::Identifier(id) if id.name == "subscribe" => id.span,
                        _ => continue,
                    },
                    _ => continue,
                };
                // Report only when the call's value is ignored — i.e. its parent is
                // an `ExpressionStatement` directly. Assignments, declarations,
                // returns, or being passed as arguments all keep the unsubscribe
                // function reachable.
                let parent_kind = nodes.parent_kind(node.id());
                if !matches!(parent_kind, AstKind::ExpressionStatement(_)) {
                    continue;
                }
                let s = content_offset + property_span.start;
                let e = content_offset + property_span.end;
                ctx.diagnostic(
                    "Ignoring returned value of the subscribe method is forbidden.",
                    Span::new(s, e),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn subscribe_property_locations_and_kept_return_values() {
        let source = "<!-- 😀 --><script data-note=\">\">store.subscribe(cb); store[subscribe](cb); store['subscribe'](cb); const stop = store.subscribe(cb); consume(store.subscribe(cb));</script><script context=\"module\" data-note=\">\">store.subscribe(cb);</script>";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let diagnostics: Vec<_> = crate::linter::Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-ignored-unsubscribe")
            .collect();
        assert_eq!(diagnostics.len(), 3);
        for diagnostic in diagnostics {
            assert_eq!(
                &source[diagnostic.span.start as usize..diagnostic.span.end as usize],
                "subscribe"
            );
            assert!(diagnostic.fix.is_none());
            assert!(diagnostic.suggestions.is_empty());
        }
    }
}
