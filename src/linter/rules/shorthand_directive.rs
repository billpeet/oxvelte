//! `svelte/shorthand-directive` — enforce use of shorthand syntax for directives.
//! 🔧 Fixable

use crate::ast::{Attribute, AttributeValue, DirectiveKind, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use oxc::ast::ast::Expression;
use oxc::span::Span;

pub struct ShorthandDirective;

impl Rule for ShorthandDirective {
    fn name(&self) -> &'static str {
        "svelte/shorthand-directive"
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
                let Attribute::Directive {
                    kind,
                    name,
                    value,
                    span,
                    ..
                } = attr
                else {
                    continue;
                };
                if !matches!(
                    kind,
                    DirectiveKind::Binding | DirectiveKind::Class | DirectiveKind::StyleDirective
                ) {
                    continue;
                }
                let region = &ctx.source[span.start as usize..span.end as usize];
                if prefer_never {
                    if !region.contains('=') {
                        let insert_at = el
                            .attribute_meta
                            .get(idx)
                            .and_then(|m| m.directive_subject_span)
                            .unwrap_or_else(|| Span::new(span.end, span.end))
                            .end;
                        ctx.diagnostic_with_fix(
                            "Expected regular directive syntax.",
                            *span,
                            Fix {
                                span: Span::new(insert_at, insert_at),
                                replacement: format!("={{{name}}}"),
                            },
                        );
                    }
                } else if let Some(eq) = region.find('=') {
                    let meta = &el.attribute_meta[idx];
                    let (raw, expression) = match value {
                        AttributeValue::Expression(expr) => (expr.as_str(), meta.expression_ast),
                        AttributeValue::Concat(parts) if parts.len() == 1 => {
                            let crate::ast::AttributeValuePart::Expression(expr) = &parts[0] else {
                                continue;
                            };
                            (
                                expr.as_str(),
                                meta.parts.first().and_then(|part| part.expression_ast),
                            )
                        }
                        _ => continue,
                    };
                    if expression_is_identifier(expression, raw, name) {
                        let fix_start = span.start + eq as u32;
                        ctx.diagnostic_with_fix(
                            "Expected shorthand directive.",
                            *span,
                            Fix {
                                span: Span::new(fix_start, span.end),
                                replacement: String::new(),
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
            .filter(|d| d.rule_name == "svelte/shorthand-directive")
            .collect()
    }

    #[test]
    fn parity_regression_preserves_source_boundaries() {
        let source = "<!-- 😀 --><div style:color = \"{color}\" class:active='{active}' />";
        let diagnostics = lint(source, serde_json::json!([]));
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics
            .iter()
            .all(|d| d.fix.as_ref().unwrap().replacement.is_empty()));
        assert!(lint("<div style:color=\" {color} \" />", serde_json::json!([])).is_empty());
    }
}
