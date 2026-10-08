//! `svelte/no-svelte-internal` — disallow importing from svelte/internal.
//! ⭐ Recommended

use crate::linter::{LintContext, Rule};
use oxc::ast::ast::{Expression, Statement};
use oxc::ast::AstKind;
use oxc::span::Span;

pub struct NoSvelteInternal;

impl Rule for NoSvelteInternal {
    fn name(&self) -> &'static str {
        "svelte/no-svelte-internal"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        for (sem, offset) in [
            (
                ctx.instance_semantic,
                ctx.ast
                    .instance
                    .as_ref()
                    .map_or(ctx.instance_content_offset, |s| s.content_span.start),
            ),
            (
                ctx.module_semantic,
                ctx.ast
                    .module
                    .as_ref()
                    .map_or(ctx.module_content_offset, |s| s.content_span.start),
            ),
        ]
        .into_iter()
        .filter_map(|(s, o)| s.map(|s| (s, o)))
        {
            for stmt in &sem.nodes().program().body {
                let source_span = match stmt {
                    Statement::ImportDeclaration(imp) => {
                        if is_svelte_internal(imp.source.value.as_str()) {
                            Some(imp.span)
                        } else {
                            None
                        }
                    }
                    Statement::ExportAllDeclaration(exp) => {
                        if is_svelte_internal(exp.source.value.as_str()) {
                            Some(exp.span)
                        } else {
                            None
                        }
                    }
                    Statement::ExportNamedDeclaration(exp) => exp.source.as_ref().and_then(|s| {
                        if is_svelte_internal(s.value.as_str()) {
                            Some(exp.span)
                        } else {
                            None
                        }
                    }),
                    _ => None,
                };
                if let Some(span) = source_span {
                    let s = offset + span.start;
                    let e = offset + span.end;
                    ctx.diagnostic(
                        "Using svelte/internal is prohibited. This will be removed in Svelte 6.",
                        Span::new(s, e),
                    );
                }
            }

            for node in sem.nodes().iter() {
                let AstKind::ImportExpression(import_expr) = node.kind() else {
                    continue;
                };
                let Expression::StringLiteral(lit) = &import_expr.source else {
                    continue;
                };
                if !is_svelte_internal(lit.value.as_str()) {
                    continue;
                }
                let s = offset + import_expr.span.start;
                let e = offset + import_expr.span.end;
                ctx.diagnostic(
                    "Using svelte/internal is prohibited. This will be removed in Svelte 6.",
                    Span::new(s, e),
                );
            }
        }
    }
}

fn is_svelte_internal(s: &str) -> bool {
    s == "svelte/internal" || s.starts_with("svelte/internal/")
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_full_import_nodes_with_unicode_and_comments() {
        let source = "<!-- é -->\n<script data-x='>'>import /* comment */ x from 'svelte/internal'; import('svelte/internal/foo'); import('svelte/internality');</script>";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        let diagnostics =
            crate::linter::Linter::all().lint_with_config(&parsed.ast, source, Default::default());
        let reads: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.rule_name == "svelte/no-svelte-internal")
            .map(|d| &source[d.span.start as usize..d.span.end as usize])
            .collect();
        assert_eq!(
            reads,
            [
                "import /* comment */ x from 'svelte/internal';",
                "import('svelte/internal/foo')"
            ]
        );
    }
}
