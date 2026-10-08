//! `svelte/shorthand-attribute` — enforce use of shorthand syntax for attributes.
//! 🔧 Fixable

use crate::ast::{Attribute, AttributeValue, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use oxc::ast::ast::Expression;

pub struct ShorthandAttribute;

impl Rule for ShorthandAttribute {
    fn name(&self) -> &'static str {
        "svelte/shorthand-attribute"
    }

    fn is_fixable(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let prefer_never = ctx
            .config
            .options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("prefer"))
            .and_then(|v| v.as_str())
            == Some("never");

        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            for (idx, attr) in el.attributes.iter().enumerate() {
                if let Attribute::NormalAttribute { name, value, span } = attr {
                    let meta = &el.attribute_meta[idx];
                    let (expr, expression) = match value {
                        AttributeValue::Expression(expr) => (expr, meta.expression_ast),
                        AttributeValue::Concat(parts) if parts.len() == 1 => {
                            let crate::ast::AttributeValuePart::Expression(expr) = &parts[0] else {
                                continue;
                            };
                            (
                                expr,
                                meta.parts.first().and_then(|part| part.expression_ast),
                            )
                        }
                        _ => continue,
                    };
                    if !prefer_never && !expression_is_identifier(expression, expr, name) {
                        continue;
                    }
                    let src = &ctx.source[span.start as usize..span.end as usize];
                    if prefer_never && src.starts_with('{') {
                        let key = expression_identifier_name(expression).unwrap_or(name.as_str());
                        ctx.diagnostic_with_fix(
                            "Expected regular attribute syntax.",
                            *span,
                            Fix {
                                span: oxc::span::Span::new(span.start, span.start),
                                replacement: format!("{key}="),
                            },
                        );
                    } else if !prefer_never && !src.starts_with('{') {
                        ctx.diagnostic_with_fix(
                            "Expected shorthand attribute.",
                            *span,
                            Fix {
                                span: *span,
                                replacement: {
                                    let mustache = meta
                                        .mustache_span
                                        .or_else(|| {
                                            meta.parts.first().and_then(|part| part.mustache_span)
                                        })
                                        .unwrap();
                                    ctx.source[mustache.start as usize..mustache.end as usize]
                                        .to_string()
                                },
                            },
                        );
                    }
                }
            }
        });
    }
}

fn expression_identifier_name<'a>(expr: Option<&'a Expression<'a>>) -> Option<&'a str> {
    match expr {
        Some(Expression::Identifier(id)) => Some(id.name.as_str()),
        _ => None,
    }
}

fn expression_is_identifier(expr: Option<&Expression>, raw: &str, expected: &str) -> bool {
    expression_identifier_name(expr).map_or_else(|| raw.trim() == expected, |name| name == expected)
}

#[cfg(test)]
mod tests {
    use crate::{
        linter::{LintDiagnostic, Linter, RuleConfig},
        parser,
    };
    use oxc::allocator::Allocator;

    fn lint(source: &str, options: serde_json::Value) -> Vec<LintDiagnostic> {
        let alloc = Allocator::default();
        let parsed = parser::parse_for_lint(source, &alloc);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint_with_config(
                &parsed.ast,
                source,
                RuleConfig {
                    options: Some(options),
                    settings: None,
                },
            )
            .into_iter()
            .filter(|d| d.rule_name == "svelte/shorthand-attribute")
            .collect()
    }

    #[test]
    fn parity_regression_preserves_source_boundaries() {
        let source = "<!-- 😀 --><div value = \"{ /* keep */ value }\" />";
        let diagnostics = lint(source, serde_json::json!([]));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].fix.as_ref().unwrap().replacement,
            "{ /* keep */ value }"
        );
        let source = "<div { /* keep */ value } />";
        let diagnostics = lint(source, serde_json::json!([{"prefer":"never"}]));
        let fix = diagnostics[0].fix.as_ref().unwrap();
        assert_eq!(fix.span.start, fix.span.end);
        assert_eq!(fix.replacement, "value=");
    }
}
