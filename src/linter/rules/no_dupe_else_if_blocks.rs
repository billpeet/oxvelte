//! Detect conditions covered by prior branches of an if/else chain.
use crate::ast::TemplateNode;
use crate::linter::{walk_template_nodes, LintContext, Rule};
use oxc::{
    allocator::Allocator,
    ast::ast::Expression,
    parser::Parser,
    span::{GetSpan, SourceType, Span},
};
use std::collections::HashMap;
pub struct NoDupeElseIfBlocks;
const MSG:&str="This branch can never execute. Its condition is a duplicate or covered by previous conditions in the `{#if}` / `{:else if}` chain.";
impl Rule for NoDupeElseIfBlocks {
    fn name(&self) -> &'static str {
        "svelte/no-dupe-else-if-blocks"
    }
    fn is_recommended(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let mut ancestors: HashMap<u32, Vec<String>> = HashMap::new();
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::IfBlock(block) = node else {
                return;
            };
            let mut previous = ancestors
                .get(&block.span.start)
                .cloned()
                .unwrap_or_default();
            if !block.test.is_empty() {
                if let Some(span) = covered(&block.test, &previous) {
                    ctx.diagnostic(
                        MSG,
                        Span::new(
                            block.test_span.start + span.start,
                            block.test_span.start + span.end,
                        ),
                    );
                }
                previous.push(block.test.clone());
            } else {
                for child in &block.consequent.nodes {
                    if let TemplateNode::IfBlock(inner) = child {
                        ancestors.insert(inner.span.start, previous.clone());
                    }
                }
            }
            if let Some(alt) = &block.alternate {
                if let TemplateNode::IfBlock(alt) = alt.as_ref() {
                    ancestors.insert(alt.span.start, previous);
                }
            }
        });
    }
}
fn split<'a>(expr: &'a Expression<'a>, op: &str, out: &mut Vec<&'a Expression<'a>>) {
    let expr = expr.without_parentheses();
    if let Expression::LogicalExpression(l) = expr {
        if l.operator.as_str() == op {
            split(&l.left, op, out);
            split(&l.right, op, out);
            return;
        }
    }
    out.push(expr);
}
fn key(source: &str, expr: &Expression<'_>) -> String {
    let expr = expr.without_parentheses();
    if let Expression::LogicalExpression(l) = expr {
        if matches!(l.operator.as_str(), "||" | "&&") {
            let mut keys = [key(source, &l.left), key(source, &l.right)];
            keys.sort();
            return format!("{}:[{}][{}]", l.operator.as_str(), keys[0], keys[1]);
        }
    }
    let span = expr.span();
    super::canonical_js_expression(&source[span.start as usize..span.end as usize])
}
fn operands(source: &str, expr: &Expression<'_>) -> Vec<Vec<String>> {
    let mut ors = Vec::new();
    split(expr, "||", &mut ors);
    ors.into_iter()
        .map(|e| {
            let mut ands = Vec::new();
            split(e, "&&", &mut ands);
            ands.into_iter().map(|e| key(source, e)).collect()
        })
        .collect()
}
fn covered(source: &str, previous: &[String]) -> Option<Span> {
    let allocator = Allocator::default();
    let expr = Parser::new(&allocator, source, SourceType::ts())
        .parse_expression()
        .ok()?;
    let mut conditions = Vec::new();
    if matches!(expr.without_parentheses(),Expression::LogicalExpression(l) if l.operator.as_str()=="&&")
    {
        split(&expr, "&&", &mut conditions);
    }
    conditions.push(expr.without_parentheses());
    let mut remaining: Vec<_> = conditions.iter().map(|e| operands(source, e)).collect();
    for prior in previous.iter().rev() {
        let Ok(prior_expr) = Parser::new(&allocator, prior, SourceType::ts()).parse_expression()
        else {
            continue;
        };
        let prior_operands = operands(prior, &prior_expr);
        for (i, list) in remaining.iter_mut().enumerate() {
            list.retain(|candidate| {
                !prior_operands
                    .iter()
                    .any(|p| p.iter().all(|key| candidate.contains(key)))
            });
            if list.is_empty() {
                return Some(conditions[i].span());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::covered;
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;
    #[test]
    fn duplicate_checks_use_js_tokens_and_cover_nested_logical_operands() {
        assert!(covered("x + ++y", &["x++ + y".into()]).is_none());
        assert!(covered("name === 'a b'", &["name === 'ab'".into()]).is_none());
        assert!(covered("a /* comment */ && b", &["b && a".into()]).is_some());
        let expression = "d && ((c && e && b) || a)";
        let span = covered(expression, &["a".into(), "b && c".into()]).unwrap();
        assert_eq!(
            &expression[span.start as usize..span.end as usize],
            "(c && e && b) || a"
        );
    }
    #[test]
    fn nested_else_reports_once_at_the_expression() {
        let source = "<!-- 😀 -->{#if ready}a{:else}text{#if ready /* note */}b{/if}{/if}";
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-dupe-else-if-blocks")
            .collect();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "ready"
        );
    }
}
