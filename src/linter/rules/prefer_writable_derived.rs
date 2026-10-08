//! `svelte/prefer-writable-derived` — prefer `$derived` with a setter over `$state` + `$effect`.
//! ⭐ Recommended 💡

use crate::linter::{Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::{Argument, AssignmentTarget, Expression, Statement};
use oxc::ast::AstKind;
use oxc::span::{GetSpan, Span};
use oxc::syntax::operator::AssignmentOperator;

pub struct PreferWritableDerived;

impl Rule for PreferWritableDerived {
    fn name(&self) -> &'static str {
        "svelte/prefer-writable-derived"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if ctx.runes_explicit_false
            || (!ctx.svelte_version.is_unknown() && !ctx.svelte_version.includes_major(5))
        {
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
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();

        for node in nodes.iter() {
            let AstKind::CallExpression(ce) = node.kind() else {
                continue;
            };
            if !is_effect_or_effect_pre(&ce.callee) {
                continue;
            }
            if ce.arguments.len() != 1 {
                continue;
            }

            let Some(body_statements) = fn_arg_block_body(&ce.arguments[0]) else {
                continue;
            };
            if body_statements.len() != 1 {
                continue;
            }
            let Statement::ExpressionStatement(es) = &body_statements[0] else {
                continue;
            };
            let Expression::AssignmentExpression(ae) = &es.expression else {
                continue;
            };
            if ae.operator != AssignmentOperator::Assign {
                continue;
            }
            let AssignmentTarget::AssignmentTargetIdentifier(id) = &ae.left else {
                continue;
            };

            let Some(sid) = scoping.get_reference(id.reference_id()).symbol_id() else {
                continue;
            };
            let decl_node_id = scoping.symbol_declaration(sid);
            let declarator = std::iter::once(decl_node_id)
                .chain(nodes.ancestor_ids(decl_node_id))
                .find_map(|nid| match nodes.kind(nid) {
                    AstKind::VariableDeclarator(d) => Some(d),
                    _ => None,
                });
            let Some(decl) = declarator else { continue };
            let Some(Expression::CallExpression(init_ce)) = &decl.init else {
                continue;
            };
            let Expression::Identifier(init_id) = &init_ce.callee else {
                continue;
            };
            if init_id.name != "$state" {
                continue;
            }

            let s = content_offset + decl.span.start;
            let e = content_offset + decl.span.end;
            // ESLint normalizes multiple edits to an encompassing replacement.
            // Preserve all text between the initializer and the effect call.
            let mut edits = [
                (
                    init_ce.span,
                    format!(
                        "$derived({})",
                        &ctx.source[(content_offset + ae.right.span().start) as usize
                            ..(content_offset + ae.right.span().end) as usize]
                    ),
                ),
                (ce.span, String::new()),
            ];
            edits.sort_by_key(|(span, _)| span.start);
            let start = content_offset + edits[0].0.start;
            let end = content_offset + edits[1].0.end;
            let replacement = format!(
                "{}{}{}",
                edits[0].1,
                &ctx.source[(content_offset + edits[0].0.end) as usize
                    ..(content_offset + edits[1].0.start) as usize],
                edits[1].1
            );
            ctx.diagnostic_with_suggestions(
                "Prefer using writable $derived instead of $state and $effect",
                Span::new(s, e),
                vec![Suggestion {
                    description: "Rewrite $state and $effect to $derived".into(),
                    fix: Fix {
                        span: Span::new(start, end),
                        replacement,
                    },
                }],
            );
        }
    }
}

fn is_effect_or_effect_pre(callee: &Expression<'_>) -> bool {
    match callee {
        Expression::Identifier(id) => id.name == "$effect",
        Expression::StaticMemberExpression(mem) => {
            matches!(&mem.object, Expression::Identifier(id) if id.name == "$effect")
                && mem.property.name == "pre"
        }
        _ => false,
    }
}

/// Extract the block body of a `() => { ... }` or `function () { ... }` argument.
/// Returns the statements slice only when the function has zero parameters and
/// a block body (not an expression body).
fn fn_arg_block_body<'a>(arg: &'a Argument<'a>) -> Option<&'a [Statement<'a>]> {
    match arg {
        Argument::ArrowFunctionExpression(a) => {
            if !a.params.items.is_empty() || a.params.rest.is_some() {
                return None;
            }
            if a.expression {
                return None;
            }
            Some(a.body.statements.as_slice())
        }
        Argument::FunctionExpression(f) => {
            if !f.params.items.is_empty() || f.params.rest.is_some() {
                return None;
            }
            f.body.as_ref().map(|b| b.statements.as_slice())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn rewrite_keeps_comments_and_uses_the_resolved_binding() {
        let source = "<!-- 😀 --><script data-note=\">\">let value = $state(0); /* keep */ $effect(() => { value = count + 1; }); function inner() { let value = 0; $effect(() => { value = 2; }); }</script>";
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty());
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/prefer-writable-derived")
            .collect();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].fix.is_none());
        let suggestion = &diagnostics[0].suggestions[0];
        let mut output = source.to_string();
        output.replace_range(
            suggestion.fix.span.start as usize..suggestion.fix.span.end as usize,
            &suggestion.fix.replacement,
        );
        assert_eq!(output, "<!-- 😀 --><script data-note=\">\">let value = $derived(count + 1); /* keep */ ; function inner() { let value = 0; $effect(() => { value = 2; }); }</script>");
    }
}
