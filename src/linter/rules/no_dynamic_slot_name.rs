//! `svelte/no-dynamic-slot-name` — disallow dynamic slot names.

use super::no_not_function_handler::resolve_handler_expression;
use crate::ast::{Attribute, AttributeValue, AttributeValuePart, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use oxc::{ast::ast::Expression, span::Span, syntax::operator::BinaryOperator};

pub struct NoDynamicSlotName;

impl Rule for NoDynamicSlotName {
    fn name(&self) -> &'static str {
        "svelte/no-dynamic-slot-name"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let mut findings = Vec::new();
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            if el.name != "slot" {
                return;
            }
            for (index, attr) in el.attributes.iter().enumerate() {
                let Attribute::NormalAttribute { name, value, span } = attr else {
                    continue;
                };
                if name != "name" {
                    continue;
                }
                let meta = &el.attribute_meta[index];
                match value {
                    AttributeValue::True => {
                        findings.push(("`<slot>` name requires a value.", *span, None))
                    }
                    AttributeValue::Static(_) => {}
                    AttributeValue::Expression(_) => {
                        if let Some(mustache) = meta.mustache_span {
                            report(
                                ctx,
                                &mut findings,
                                el.attribute_expression_ast(index),
                                mustache,
                                meta.value_full_span.unwrap_or(mustache),
                                true,
                            );
                        }
                    }
                    AttributeValue::Concat(parts) => {
                        for (part_index, part) in parts.iter().enumerate() {
                            if !matches!(part, AttributeValuePart::Expression(_)) {
                                continue;
                            }
                            let part_meta = &meta.parts[part_index];
                            if let Some(mustache) = part_meta.mustache_span {
                                report(
                                    ctx,
                                    &mut findings,
                                    el.attribute_part_expression_ast(index, part_index),
                                    mustache,
                                    if parts.len() == 1 {
                                        meta.value_full_span.unwrap_or(mustache)
                                    } else {
                                        mustache
                                    },
                                    parts.len() == 1,
                                );
                            }
                        }
                    }
                }
            }
        });
        for (message, span, fix) in findings {
            if let Some(fix) = fix {
                ctx.diagnostic_with_fix(message, span, fix);
            } else {
                ctx.diagnostic(message, span);
            }
        }
    }
}

fn report<'a>(
    ctx: &LintContext<'a>,
    findings: &mut Vec<(&'static str, Span, Option<Fix>)>,
    expression: Option<&'a Expression<'a>>,
    span: Span,
    fix_span: Span,
    quote: bool,
) {
    let text = expression.and_then(|expr| static_text(resolve_handler_expression(expr, ctx, span)));
    let fix = text.map(|text| Fix {
        span: fix_span,
        replacement: if quote { format!("\"{text}\"") } else { text },
    });
    findings.push(("`<slot>` name cannot be dynamic.", span, fix));
}

// Upstream getStringIfConstant accepts only strings, string-only template
// interpolation and string addition. It does not stringify numbers or booleans.
fn static_text(expr: &Expression<'_>) -> Option<String> {
    match expr {
        Expression::StringLiteral(value) => Some(value.value.to_string()),
        Expression::TemplateLiteral(template) => {
            let mut text = String::new();
            for (index, quasi) in template.quasis.iter().enumerate() {
                text.push_str(quasi.value.cooked.as_ref().map_or("null", |s| s.as_str()));
                if let Some(expression) = template.expressions.get(index) {
                    text.push_str(&static_text(expression)?);
                }
            }
            Some(text)
        }
        Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
            Some(static_text(&binary.left)? + &static_text(&binary.right)?)
        }
        Expression::ParenthesizedExpression(inner) => static_text(&inner.expression),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        linter::{Fix, LintDiagnostic, Linter},
        parser,
    };
    use oxc::allocator::Allocator;
    fn lint(source: &str) -> Vec<LintDiagnostic> {
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-dynamic-slot-name")
            .collect()
    }
    fn apply_fixes(source: &str, fixes: &[Fix]) -> String {
        let mut fixed = source.to_string();
        let mut fixes: Vec<_> = fixes.iter().collect();
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.start));
        for fix in fixes {
            fixed.replace_range(
                fix.span.start as usize..fix.span.end as usize,
                &fix.replacement,
            );
        }
        fixed
    }
    #[test]
    fn replaces_whole_quoted_values_and_each_concatenated_mustache() {
        let source = "<!-- 😀 --><script data-note=\">\">const name = 'row';</script><slot name=\"{name}\"/><slot name='pre{\"a\"}{\"b\"}'/>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3);
        let fixes: Vec<_> = diagnostics.iter().map(|d| d.fix.clone().unwrap()).collect();
        assert_eq!(apply_fixes(source, &fixes), "<!-- 😀 --><script data-note=\">\">const name = 'row';</script><slot name=\"row\"/><slot name='preab'/>");
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "{name}"
        );
    }
    #[test]
    fn string_only_constant_evaluation_matches_upstream() {
        let diagnostics =
            lint("<slot name={'a' + `b${'c'}`}/><slot name={1}/><slot name={`a${1}`}/>");
        assert_eq!(diagnostics.len(), 3);
        assert_eq!(diagnostics[0].fix.as_ref().unwrap().replacement, "\"abc\"");
        assert!(diagnostics[1].fix.is_none());
        assert!(diagnostics[2].fix.is_none());
    }
    #[test]
    fn cycles_and_template_shadows_remain_dynamic_without_fixes() {
        let source = "<script>const first = second; const second = first; const name = 'outer';</script><slot name={first}/>{#each items as name}<slot name={name}/>{:else}<slot name={name}/>{/each}";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3);
        assert!(diagnostics[0].fix.is_none());
        assert!(diagnostics[1].fix.is_none());
        assert_eq!(
            diagnostics[2].fix.as_ref().unwrap().replacement,
            "\"outer\""
        );
    }
}
