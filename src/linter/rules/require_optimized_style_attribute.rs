//! `svelte/require-optimized-style-attribute` — require use of optimized style attribute syntax.

use crate::ast::{Attribute, AttributeValue, AttributeValuePart, TemplateNode};
use crate::linter::{walk_template_nodes, LintContext, Rule};

const TOO_COMPLEX: &str = "It cannot be optimized because too complex.";

pub struct RequireOptimizedStyleAttribute;

impl Rule for RequireOptimizedStyleAttribute {
    fn name(&self) -> &'static str {
        "svelte/require-optimized-style-attribute"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            for attr in &el.attributes {
                if let Attribute::NormalAttribute { name, value, span } = attr {
                    if name == "style" {
                        if let Some(reason) = unoptimized_reason(value) {
                            let raw = &ctx.source[span.start as usize..span.end as usize];
                            let relative = if reason.contains("comments") {
                                raw.find("/*").unwrap_or(0)
                            } else if reason.contains("property of") {
                                raw.match_indices('{')
                                    .find_map(|(start, _)| {
                                        let end =
                                            super::style_declarations::expression_end(raw, start);
                                        raw[end..].trim_start().starts_with(':').then_some(start)
                                    })
                                    .unwrap_or(0)
                            } else {
                                raw.match_indices('{')
                                    .find_map(|(start, _)| {
                                        let prefix = &raw[..start];
                                        let last =
                                            prefix.rsplit(';').next().unwrap_or(prefix).trim();
                                        (!last.contains(':')).then_some(start)
                                    })
                                    .unwrap_or(0)
                            };
                            let end = if reason.contains("comments") {
                                raw[relative + 2..]
                                    .find("*/")
                                    .map_or(raw.len(), |n| relative + n + 4)
                            } else if raw.as_bytes().get(relative) == Some(&b'{') {
                                super::style_declarations::expression_end(raw, relative)
                            } else {
                                raw.len()
                            };
                            ctx.diagnostic(
                                reason,
                                oxc::span::Span::new(
                                    span.start + relative as u32,
                                    span.start + end as u32,
                                ),
                            );
                        }
                    }
                }
            }
        });
    }
}

fn get_static(part: &AttributeValuePart) -> Option<&str> {
    if let AttributeValuePart::Static(s) = part {
        Some(s.as_str())
    } else {
        None
    }
}

fn unoptimized_reason(value: &AttributeValue) -> Option<&'static str> {
    match value {
        AttributeValue::Expression(_) => Some(TOO_COMPLEX),
        AttributeValue::Concat(parts) => {
            let static_text: String = parts.iter().filter_map(|p| get_static(p)).collect();
            if !static_text.contains(':') {
                return Some(TOO_COMPLEX);
            }
            if static_text.contains("/*") {
                return Some("It cannot be optimized because contains comments.");
            }

            for (i, part) in parts.iter().enumerate() {
                if !matches!(part, AttributeValuePart::Expression(_)) {
                    continue;
                }
                let before = (i > 0)
                    .then(|| parts.get(i - 1))
                    .flatten()
                    .and_then(|p| get_static(p));
                let after = parts.get(i + 1).and_then(|p| get_static(p));
                if after.map_or(false, |a| a.trim_start().starts_with(':')) {
                    return Some("It cannot be optimized because property of style declaration contain interpolation.");
                }
                if let Some(b) = before {
                    let t = b.trim_end();
                    if (t.ends_with(';') || t.is_empty())
                        && !after.map_or(false, |a| {
                            let s = a.trim_start();
                            s.starts_with(';') || s.starts_with('}')
                        })
                    {
                        return Some(TOO_COMPLEX);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_comment_and_dynamic_property_spans_after_unicode() {
        for (source, expected) in [
            (
                "<div title='é' style=\"color:{value}; /* é */ padding:0\"/>",
                "/* é */",
            ),
            (
                "<div title='é' style=\"color:{value}; {key}: red\"/>",
                "{key}",
            ),
            (
                "<div title='é' style=\"color:{value}; {styles}\"/>",
                "{styles}",
            ),
        ] {
            let allocator = oxc::allocator::Allocator::default();
            let parsed = crate::parser::parse_for_lint(source, &allocator);
            let mut ctx = LintContext::new(&parsed.ast, source);
            RequireOptimizedStyleAttribute.run(&mut ctx);
            let diagnostics = ctx.into_diagnostics();
            assert_eq!(diagnostics.len(), 1);
            let span = diagnostics[0].span;
            assert_eq!(&source[span.start as usize..span.end as usize], expected);
        }
    }
}
