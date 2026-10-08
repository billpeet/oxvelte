//! `svelte/prefer-attribute-interpolation` prefers markup interpolation for string templates.

use crate::ast::{Attribute, AttributeValue, AttributeValuePart, TemplateNode};
use crate::linter::{walk_template_nodes, LintContext, Rule};
use crate::parser::expression::{parse_template_expression, unwrap_template_expression};
use oxc::allocator::Allocator;
use oxc::ast::ast::Expression;

pub struct PreferAttributeInterpolation;

impl Rule for PreferAttributeInterpolation {
    fn name(&self) -> &'static str {
        "svelte/prefer-attribute-interpolation"
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            for (attribute, meta) in el.attributes.iter().zip(&el.attribute_meta) {
                let Attribute::NormalAttribute { value, .. } = attribute else {
                    continue;
                };
                let (text, span) = match value {
                    AttributeValue::Expression(text) => (text, meta.mustache_span),
                    AttributeValue::Concat(parts) if parts.len() == 1 => {
                        let AttributeValuePart::Expression(text) = &parts[0] else {
                            continue;
                        };
                        (text, meta.parts[0].mustache_span)
                    }
                    _ => continue,
                };
                let Some(span) = span else { continue };
                let allocator = Allocator::default();
                let parsed = parse_template_expression(text, &allocator);
                if !parsed.errors.is_empty() || !parsed.program.comments.is_empty() {
                    continue;
                }
                let Some(Expression::TemplateLiteral(template)) =
                    unwrap_template_expression(&parsed).map(Expression::get_inner_expression)
                else {
                    continue;
                };
                if template.expressions.is_empty()
                    || template.quasis.iter().any(|quasi| {
                        let raw = quasi.value.raw.as_str();
                        raw.contains(['\n', '\r', '{']) || useful_escape(raw)
                    })
                {
                    continue;
                }
                ctx.diagnostic(
                    "Prefer attribute interpolation over a template literal.",
                    span,
                );
            }
        });
    }
}

fn useful_escape(raw: &str) -> bool {
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.next().is_none_or(|ch| "nrvtbfux".contains(ch)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn flags_only_single_template_values_without_comments_or_useful_escapes() {
        let source = "<!-- 😀 --><div a={`prefix${value}`} b=\"{`text${value}`}\" c=\"prefix {`text${value}`}\" d={`text${/* keep */ value}`} e={`line\\n${value}`} f={`text{${value}`} style:color={`rgb(${value})`} />";
        let alloc = Allocator::default();
        let parsed = parser::parse_for_lint(source, &alloc);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/prefer-attribute-interpolation")
            .collect();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "{`prefix${value}`}"
        );
        assert!(diagnostics
            .iter()
            .all(|d| d.fix.is_none() && d.suggestions.is_empty()));
    }
}
