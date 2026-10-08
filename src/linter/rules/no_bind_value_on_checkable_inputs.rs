//! `svelte/no-bind-value-on-checkable-inputs` suggests checked/group bindings.

use crate::ast::{Attribute, AttributeValue, AttributeValuePart, DirectiveKind, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::Expression;
use oxc::span::Span;

pub struct NoBindValueOnCheckableInputs;

impl Rule for NoBindValueOnCheckableInputs {
    fn name(&self) -> &'static str {
        "svelte/no-bind-value-on-checkable-inputs"
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            if el.name != "input" {
                return;
            }
            let Some((attribute, meta)) = el.attributes.iter().zip(&el.attribute_meta)
                .find(|(attr, _)| matches!(attr, Attribute::NormalAttribute { name, .. } if name.eq_ignore_ascii_case("type"))) else { return };
            let Attribute::NormalAttribute { value, .. } = attribute else {
                return;
            };
            let input_type = match value {
                AttributeValue::Static(value) => Some(value.clone()),
                AttributeValue::Expression(_) => meta.expression_ast.and_then(|expression| {
                    static_string(
                        expression,
                        ctx,
                        meta.expression_span.unwrap_or(meta.name_span),
                        0,
                    )
                }),
                AttributeValue::Concat(parts) if parts.len() == 1 => match &parts[0] {
                    AttributeValuePart::Expression(_) => {
                        meta.parts[0].expression_ast.and_then(|expression| {
                            static_string(
                                expression,
                                ctx,
                                meta.parts[0].expression_span.unwrap_or(meta.parts[0].span),
                                0,
                            )
                        })
                    }
                    AttributeValuePart::Static(value) => Some(value.clone()),
                },
                _ => None,
            };
            let Some(input_type) = input_type else { return };
            let input_type = input_type.to_ascii_lowercase();
            if input_type != "checkbox" && input_type != "radio" {
                return;
            }
            let Some((binding, meta)) = el.attributes.iter().zip(&el.attribute_meta).find(|(attr, _)|
                matches!(attr, Attribute::Directive { kind: DirectiveKind::Binding, name, .. } if name == "value")) else { return };
            let Attribute::Directive { span, .. } = binding else {
                return;
            };
            let shorthand = meta.equals_span.is_none();
            let key_span = Span::new(
                span.start,
                meta.directive_subject_span.unwrap_or(meta.name_span).end,
            );
            let suggestions = if input_type == "checkbox" {
                &["checked", "group"][..]
            } else {
                &["group"][..]
            };
            let suggestions = suggestions
                .iter()
                .map(|target| Suggestion {
                    description: format!("Change `bind:value` to `bind:{target}`."),
                    fix: Fix {
                        span: if shorthand { *span } else { key_span },
                        replacement: if shorthand {
                            format!("bind:{target}={{value}}")
                        } else {
                            format!("bind:{target}")
                        },
                    },
                })
                .collect();
            let message = if input_type == "checkbox" {
                "`bind:value` does not work on checkbox inputs. Did you mean `bind:checked` or `bind:group`?"
            } else {
                "`bind:value` does not work on radio inputs. Did you mean `bind:group`?"
            };
            ctx.diagnostic_with_suggestions(message, *span, suggestions);
        });
    }
}

fn static_string<'a>(
    expression: &'a Expression<'a>,
    ctx: &LintContext<'a>,
    span: Span,
    depth: u8,
) -> Option<String> {
    if depth > 16 {
        return None;
    }
    let expression = super::no_not_function_handler::resolve_handler_expression(
        expression.without_parentheses(),
        ctx,
        span,
    );
    match expression.without_parentheses() {
        Expression::StringLiteral(literal) => Some(literal.value.to_string()),
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
            .quasis
            .first()?
            .value
            .cooked
            .as_ref()
            .map(ToString::to_string),
        Expression::BinaryExpression(binary)
            if binary.operator == oxc::syntax::operator::BinaryOperator::Addition =>
        {
            Some(format!(
                "{}{}",
                static_string(&binary.left, ctx, span, depth + 1)?,
                static_string(&binary.right, ctx, span, depth + 1)?
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn offers_independent_suggestions_preserving_binding_trivia() {
        let source = "<!-- 😀 --><script>const checkType = 'checkbox'; const radioType = 'radio'; const aliasType = radioType;</script><input TYPE={'CHECK' + 'BOX'} bind:value = \"{value}\" /><input type={`radio`} bind:value /><input type={dynamic} bind:value /><input type={('checkbox' as string)} bind:value /><input type={checkType} bind:value /><input type={aliasType} bind:value />{#each types as checkType}<input type={checkType} bind:value />{/each}";
        let alloc = Allocator::default();
        let parsed = parser::parse_for_lint(source, &alloc);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-bind-value-on-checkable-inputs")
            .collect();
        assert_eq!(diagnostics.len(), 4);
        assert_eq!(diagnostics[0].suggestions.len(), 2);
        assert_eq!(diagnostics[1].suggestions.len(), 1);
        let fix = &diagnostics[0].suggestions[0].fix;
        assert_eq!(
            &source[fix.span.start as usize..fix.span.end as usize],
            "bind:value"
        );
        assert_eq!(fix.replacement, "bind:checked");
        assert_eq!(
            diagnostics[1].suggestions[0].fix.replacement,
            "bind:group={value}"
        );
        assert!(diagnostics.iter().all(|d| d.fix.is_none()));
    }
}
