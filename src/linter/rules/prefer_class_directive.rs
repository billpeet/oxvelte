//! Prefer class directives for conditional class names.
use crate::ast::{Attribute, AttributeValue, AttributeValuePart, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use oxc::{
    allocator::Allocator,
    ast::ast::Expression,
    parser::Parser,
    span::{GetSpan, SourceType, Span},
};

pub struct PreferClassDirective;
impl Rule for PreferClassDirective {
    fn name(&self) -> &'static str {
        "svelte/prefer-class-directive"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let empty = ctx
            .config
            .options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.get("prefer"))
            .and_then(|v| v.as_str())
            != Some("always");
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            if !matches!(
                el.kind(),
                crate::ast::ElementKind::Html
                    | crate::ast::ElementKind::SvelteSpecial(crate::ast::SvelteSpecial::Element)
            ) {
                return;
            }
            for (attribute_index, attr) in el.attributes.iter().enumerate() {
                let Attribute::NormalAttribute { name, value, span } = attr else {
                    continue;
                };
                if name != "class" {
                    continue;
                }
                let parts: Vec<_> = match value {
                    AttributeValue::Expression(e) => {
                        vec![AttributeValuePart::Expression(e.clone())]
                    }
                    AttributeValue::Concat(p) => p.clone(),
                    _ => continue,
                };
                let raw = &ctx.source[span.start as usize..span.end as usize];
                let Some(meta) = el.attribute_meta.get(attribute_index) else {
                    continue;
                };
                let spans: Vec<_> = if matches!(value, AttributeValue::Expression(_)) {
                    meta.mustache_span.into_iter().collect()
                } else {
                    meta.parts
                        .iter()
                        .map(|part| part.mustache_span.unwrap_or(part.span))
                        .collect()
                };
                let ranges: Vec<_> = spans
                    .into_iter()
                    .map(|s| {
                        (
                            (s.start - span.start) as usize,
                            (s.end - span.start) as usize,
                        )
                    })
                    .collect();
                if ranges.len() != parts.len() {
                    continue;
                }
                for (i, part) in parts.iter().enumerate() {
                    let AttributeValuePart::Expression(source) = part else {
                        continue;
                    };
                    let allocator = Allocator::default();
                    let Ok(expr) =
                        Parser::new(&allocator, source, SourceType::ts()).parse_expression()
                    else {
                        continue;
                    };
                    let Expression::ConditionalExpression(c) = expr.without_parentheses() else {
                        continue;
                    };
                    let (Some(t), Some(f)) = (constant(&c.consequent), constant(&c.alternate))
                    else {
                        continue;
                    };
                    if empty && !t.trim().is_empty() && !f.trim().is_empty() {
                        continue;
                    }
                    let before = boundary(&parts, i, true);
                    let after = boundary(&parts, i, false);
                    if [t.as_str(), f.as_str()].iter().any(|s| {
                        if s.is_empty() {
                            !before && !after
                        } else {
                            !s.trim()
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                                || (!s.starts_with(char::is_whitespace) && !after)
                                || (!s.ends_with(char::is_whitespace) && !before)
                        }
                    }) {
                        continue;
                    }
                    let mut directives = Vec::new();
                    if !t.trim().is_empty() {
                        directives.push(format!(
                            "class:{}={{{}}}",
                            t.trim(),
                            text(source, &c.test)
                        ));
                    }
                    if !f.trim().is_empty() {
                        directives.push(format!(
                            "class:{}={{{}}}",
                            f.trim(),
                            negate(source, &c.test)
                        ));
                    }
                    let (start, end) = ranges[i];
                    let mut left = start;
                    let mut right = end;
                    let mut before_count = i;
                    let mut after_count = parts.len() - i - 1;
                    for j in (0..i).rev() {
                        let AttributeValuePart::Static(s) = &parts[j] else {
                            break;
                        };
                        let original = &raw[ranges[j].0..ranges[j].1];
                        left = ranges[j].1 - (original.len() - original.trim_end().len());
                        if s.trim().is_empty() {
                            before_count -= 1
                        } else {
                            break;
                        }
                    }
                    for j in i + 1..parts.len() {
                        let AttributeValuePart::Static(s) = &parts[j] else {
                            break;
                        };
                        let original = &raw[ranges[j].0..ranges[j].1];
                        right = ranges[j].0 + (original.len() - original.trim_start().len());
                        if s.trim().is_empty() {
                            after_count -= 1
                        } else {
                            break;
                        }
                    }
                    let replacement = if before_count == 0 && after_count == 0 {
                        directives.join(" ")
                    } else {
                        let separator = if before_count > 0 && after_count > 0 {
                            if t.trim().is_empty() {
                                &t
                            } else if f.trim().is_empty() {
                                &f
                            } else {
                                " "
                            }
                        } else {
                            ""
                        };
                        let separator =
                            if before_count > 0 && after_count > 0 && separator.is_empty() {
                                " "
                            } else {
                                separator
                            };
                        format!(
                            "{}{}{} {}",
                            &raw[..left],
                            separator,
                            &raw[right..],
                            directives.join(" ")
                        )
                    };
                    ctx.diagnostic_with_fix(
                        "Unexpected class using the ternary operator.",
                        Span::new(span.start + start as u32, span.start + end as u32),
                        Fix {
                            span: *span,
                            replacement,
                        },
                    );
                }
            }
        });
    }
}
fn text<'a>(source: &'a str, expr: &Expression<'_>) -> &'a str {
    let s = expr.span();
    &source[s.start as usize..s.end as usize]
}
fn constant(expr: &Expression<'_>) -> Option<String> {
    match expr.without_parentheses() {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
            Some(t.quasis.first()?.value.cooked.as_ref()?.to_string())
        }
        _ => None,
    }
}
fn boundary(parts: &[AttributeValuePart], i: usize, before: bool) -> bool {
    let indices: Vec<usize> = if before {
        (0..i).rev().collect()
    } else {
        (i + 1..parts.len()).collect()
    };
    for j in indices {
        let strings = match &parts[j] {
            AttributeValuePart::Static(s) => vec![s.clone()],
            AttributeValuePart::Expression(e) => {
                let a = Allocator::default();
                let Ok(expr) = Parser::new(&a, e, SourceType::ts()).parse_expression() else {
                    return false;
                };
                let Some(strings) = possible_strings(&expr) else {
                    return false;
                };
                strings
            }
        };
        if strings.iter().any(|s| {
            !s.is_empty()
                && !(if before {
                    s.ends_with(char::is_whitespace)
                } else {
                    s.starts_with(char::is_whitespace)
                })
        }) {
            return false;
        }
        if strings.iter().all(|s| !s.is_empty()) {
            return true;
        }
    }
    true
}
fn possible_strings(expr: &Expression<'_>) -> Option<Vec<String>> {
    if let Expression::ConditionalExpression(c) = expr.without_parentheses() {
        let mut values = possible_strings(&c.consequent)?;
        values.extend(possible_strings(&c.alternate)?);
        Some(values)
    } else {
        Some(vec![constant(expr)?])
    }
}
fn negate(source: &str, expr: &Expression<'_>) -> String {
    let expr = expr.without_parentheses();
    match expr {
        Expression::UnaryExpression(u) if u.operator.as_str() == "!" => {
            text(source, u.argument.without_parentheses()).into()
        }
        Expression::BinaryExpression(b)
            if matches!(b.operator.as_str(), "==" | "===" | "!=" | "!==") =>
        {
            let op = b.operator.as_str();
            let inverse = match op {
                "==" => "!=",
                "===" => "!==",
                "!=" => "==",
                _ => "===",
            };
            let left_end = b.left.span().end as usize;
            let right_start = b.right.span().start as usize;
            let Some(token) = super::script_token_spans(source).into_iter().find(|token| {
                token.start as usize >= left_end
                    && token.end as usize <= right_start
                    && &source[token.start as usize..token.end as usize] == op
            }) else {
                return format!("!({})", text(source, expr));
            };
            format!(
                "{}{}{}{}{}",
                text(source, &b.left),
                &source[left_end..token.start as usize],
                inverse,
                &source[token.end as usize..right_start],
                text(source, &b.right)
            )
        }
        Expression::Identifier(_)
        | Expression::CallExpression(_)
        | Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_) => format!("!{}", text(source, expr)),
        _ => format!("!({})", text(source, expr)),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        linter::{Linter, RuleConfig},
        parser,
    };
    use oxc::allocator::Allocator;
    #[test]
    fn ranges_and_negation_preserve_entities_and_operator_comments() {
        let source =
            "<!-- 😀 --><button class=\"&amp; {a /* === */ === b ? '' : 'active'} tail\" />";
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/prefer-class-directive")
            .collect();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "{a /* === */ === b ? '' : 'active'}"
        );
        assert_eq!(
            diagnostics[0].fix.as_ref().unwrap().replacement,
            "class=\"&amp; tail\" class:active={a /* === */ !== b}"
        );
        let source = "<button class=\"{flag ? ' ' : 'suffix'}{ready ? 'active' : ''}\" />";
        let parsed = parser::parse_for_lint(source, &allocator);
        assert!(Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .all(|d| d.rule_name != "svelte/prefer-class-directive"));
    }
    #[test]
    fn class_edits_preserve_unicode_comments_and_unknown_neighbors() {
        let source =
            "<!-- 😀 --><button class=\"fixed {ready /* comment */ ? 'active' : ''} tail\" />";
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        let diagnostics = Linter::all().lint(&parsed.ast, source);
        let diagnostics: Vec<_> = diagnostics
            .into_iter()
            .filter(|d| d.rule_name == "svelte/prefer-class-directive")
            .collect();
        assert_eq!(diagnostics.len(), 1);
        let d = &diagnostics[0];
        assert_eq!(
            &source[d.span.start as usize..d.span.end as usize],
            "{ready /* comment */ ? 'active' : ''}"
        );
        assert_eq!(
            d.fix.as_ref().unwrap().replacement,
            "class=\"fixed tail\" class:active={ready}"
        );
        let source = "<button class=\"{unknown}{ready ? 'active' : ''}\" />";
        let parsed = parser::parse_for_lint(source, &allocator);
        assert!(Linter::all()
            .lint_with_config(&parsed.ast, source, RuleConfig::default())
            .into_iter()
            .all(|d| d.rule_name != "svelte/prefer-class-directive"));
    }
}
