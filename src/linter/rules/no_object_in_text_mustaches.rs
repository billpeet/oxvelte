//! `svelte/no-object-in-text-mustaches` — disallow objects in text mustache interpolation.
//! ⭐ Recommended

use crate::ast::{Attribute, AttributeValue, AttributeValuePart, TemplateNode};
use crate::linter::{walk_template_nodes, LintContext, Rule};

pub struct NoObjectInTextMustaches;

impl Rule for NoObjectInTextMustaches {
    fn name(&self) -> &'static str {
        "svelte/no-object-in-text-mustaches"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        walk_template_nodes(&ctx.ast.html, &mut |node| match node {
            TemplateNode::MustacheTag(tag) => {
                let kind = tag.expression_ast.and_then(detect_expression_kind);
                if let Some(label) = kind {
                    ctx.diagnostic(
                        format!("Unexpected {} in text mustache interpolation.", label),
                        tag.span,
                    );
                }
            }
            TemplateNode::Element(el) => {
                for (attr, meta) in el.attributes.iter().zip(&el.attribute_meta) {
                    if let Attribute::NormalAttribute {
                        value: AttributeValue::Concat(parts),
                        ..
                    } = attr
                    {
                        for (part, part_meta) in parts.iter().zip(&meta.parts) {
                            if let AttributeValuePart::Expression(_) = part {
                                if let Some(label) =
                                    part_meta.expression_ast.and_then(detect_expression_kind)
                                {
                                    ctx.diagnostic(
                                        format!(
                                            "Unexpected {} in text mustache interpolation.",
                                            label
                                        ),
                                        part_meta.mustache_span.unwrap_or(part_meta.span),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        });
    }
}

fn detect_expression_kind(expr: &oxc::ast::ast::Expression<'_>) -> Option<&'static str> {
    use oxc::ast::ast::Expression;
    match expr.get_inner_expression() {
        Expression::ObjectExpression(_) => Some("object"),
        Expression::ArrayExpression(_) => Some("array"),
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
            Some("function")
        }
        Expression::ClassExpression(_) => Some("class"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn reports_each_concatenated_expression_at_its_own_span() {
        let source =
            "<!-- 😀 --><div text=\"prefix {[1]} {({a: 1})}\" prop={{a: 1}} />{[1].length}";
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-object-in-text-mustaches")
            .collect();
        assert_eq!(diagnostics.len(), 2);
        let spans: Vec<_> = diagnostics
            .iter()
            .map(|d| &source[d.span.start as usize..d.span.end as usize])
            .collect();
        assert_eq!(spans, ["{[1]}", "{({a: 1})}"]);
    }
}
