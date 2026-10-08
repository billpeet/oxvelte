//! Prefer style directives for convertible CSS declarations.
use crate::ast::{Attribute, AttributeValue, AttributeValuePart, DirectiveKind, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use oxc::{
    allocator::Allocator,
    ast::ast::Expression,
    parser::Parser,
    span::{GetSpan, SourceType, Span},
};
pub struct PreferStyleDirective;
struct Item {
    start: usize,
    end: usize,
    directive: Option<String>,
    diagnostic: (usize, usize),
}
impl Rule for PreferStyleDirective {
    fn name(&self) -> &'static str {
        "svelte/prefer-style-directive"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
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
                if name != "style" {
                    continue;
                }
                let raw = &ctx.source[span.start as usize..span.end as usize];
                let Some(eq) = raw.find('=') else { continue };
                let mut begin = eq + 1;
                while raw
                    .as_bytes()
                    .get(begin)
                    .is_some_and(u8::is_ascii_whitespace)
                {
                    begin += 1;
                }
                let quoted = matches!(raw.as_bytes().get(begin), Some(b'\'' | b'"'));
                let end = raw.len() - usize::from(quoted);
                begin += usize::from(quoted);
                let Some(meta) = el.attribute_meta.get(attribute_index) else {
                    continue;
                };
                let mut interpolations = Vec::new();
                match value {
                    AttributeValue::Expression(expr) => {
                        if let (Some(mustache), Some(expression)) =
                            (meta.mustache_span, meta.expression_span)
                        {
                            interpolations.push((
                                (mustache.start - span.start) as usize,
                                (mustache.end - span.start) as usize,
                                (expression.start - span.start) as usize,
                                expr.as_str(),
                            ));
                        }
                    }
                    AttributeValue::Concat(parts) => {
                        for (part, metadata) in parts.iter().zip(&meta.parts) {
                            if let (
                                AttributeValuePart::Expression(expr),
                                Some(mustache),
                                Some(expression),
                            ) = (part, metadata.mustache_span, metadata.expression_span)
                            {
                                interpolations.push((
                                    (mustache.start - span.start) as usize,
                                    (mustache.end - span.start) as usize,
                                    (expression.start - span.start) as usize,
                                    expr.as_str(),
                                ));
                            }
                        }
                    }
                    AttributeValue::Static(_) => {}
                    _ => continue,
                }
                let mut items = Vec::new();
                let mut start = begin;
                let mut i = begin;
                let mut parens = 0;
                let mut quote = None;
                while i < end {
                    if !raw.as_bytes()[i].is_ascii() {
                        i += raw[i..end].chars().next().unwrap().len_utf8();
                        continue;
                    }
                    if quote.is_none() && raw[i..end].starts_with("/*") {
                        let Some(close) = raw[i + 2..end].find("*/") else {
                            break;
                        };
                        i += close + 4;
                        continue;
                    }
                    if let Some((_, close, expr_start, expr)) =
                        interpolations.iter().find(|p| p.0 == i)
                    {
                        if raw[start..i].trim().is_empty() {
                            let (directive, diagnostic) = inline(expr, *expr_start);
                            items.push(Item {
                                start: i,
                                end: *close,
                                directive,
                                diagnostic,
                            });
                            i = *close;
                            start = i;
                            continue;
                        }
                        i = *close;
                        continue;
                    }
                    let b = raw.as_bytes()[i];
                    if let Some(q) = quote {
                        if b == b'\\' {
                            i += 1;
                            i += raw[i..end].chars().next().map_or(0, char::len_utf8);
                            continue;
                        }
                        if b == q {
                            quote = None
                        }
                    } else {
                        match b {
                            b'\'' | b'"' => quote = Some(b),
                            b'(' => parens += 1,
                            b')' => parens -= 1,
                            b';' if parens == 0 => {
                                if let Some(item) = declaration(raw, start, i + 1, &interpolations)
                                {
                                    items.push(item)
                                }
                                start = i + 1;
                            }
                            _ => {}
                        }
                    }
                    i += 1;
                }
                if let Some(item) = declaration(raw, start, end, &interpolations) {
                    items.push(item)
                }
                for (index, item) in items.iter().enumerate() {
                    let Some(directive) = &item.directive else {
                        continue;
                    };
                    let prop = directive
                        .strip_prefix("style:")
                        .unwrap()
                        .split('=')
                        .next()
                        .unwrap();
                    if el.attributes.iter().any(|a| matches!(a,Attribute::Directive {kind:DirectiveKind::StyleDirective,name,..} if name==prop)) {continue}
                    let (fix_span, replacement) = if items.len() == 1 {
                        (*span, directive.clone())
                    } else {
                        let (remove_start, remove_end) = if let Some(after) = items.get(index + 1) {
                            (item.start, after.start)
                        } else {
                            (items[index - 1].end, item.end)
                        };
                        if index == 0 {
                            (
                                Span::new(span.start, span.start + remove_end as u32),
                                format!("{directive} {}", &raw[..remove_start]),
                            )
                        } else {
                            (
                                Span::new(span.start + remove_start as u32, span.end),
                                format!("{} {directive}", &raw[remove_end..]),
                            )
                        }
                    };
                    ctx.diagnostic_with_fix(
                        "Can use style directives instead.",
                        Span::new(
                            span.start + item.diagnostic.0 as u32,
                            span.start + item.diagnostic.1 as u32,
                        ),
                        Fix {
                            span: fix_span,
                            replacement,
                        },
                    );
                }
            }
        });
    }
}
fn declaration(
    raw: &str,
    start: usize,
    end: usize,
    interpolations: &[(usize, usize, usize, &str)],
) -> Option<Item> {
    let text = &raw[start..end];
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let start = start + (text.len() - text.trim_start().len());
    let end = start + trimmed.len();
    let directive = (|| {
        let colon = raw[start..end].find(':')? + start;
        if interpolations
            .iter()
            .any(|(s, e, ..)| *s < colon && *e > start)
        {
            return None;
        }
        let prop = raw[start..colon].trim();
        if prop.is_empty()
            || !prop
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        let value = raw[colon + 1..end].trim_end_matches(';').trim();
        if value.is_empty() || value.to_ascii_lowercase().contains("!important") {
            return None;
        }
        let value_start =
            colon + 1 + (raw[colon + 1..end].len() - raw[colon + 1..end].trim_start().len());
        let mut escaped = String::new();
        for (offset, c) in value.char_indices() {
            if c == '"'
                && !interpolations.iter().any(|(start, end, ..)| {
                    value_start + offset >= *start && value_start + offset < *end
                })
            {
                escaped.push_str("&quot;")
            } else {
                escaped.push(c)
            }
        }
        Some(format!("style:{prop}=\"{escaped}\""))
    })();
    Some(Item {
        start,
        end,
        directive,
        diagnostic: (start, end),
    })
}
fn inline(expr: &str, start: usize) -> (Option<String>, (usize, usize)) {
    let allocator = Allocator::default();
    let Ok(parsed) = Parser::new(&allocator, expr, SourceType::ts()).parse_expression() else {
        return (None, (start, start + expr.len()));
    };
    let Expression::ConditionalExpression(c) = parsed.without_parentheses() else {
        return (None, (start, start + expr.len()));
    };
    let diagnostic = (start + c.span.start as usize, start + c.span.end as usize);
    let (Expression::StringLiteral(t), Expression::StringLiteral(f)) =
        (&c.consequent, &c.alternate)
    else {
        return (None, diagnostic);
    };
    if !t.value.is_empty() && !f.value.is_empty() {
        return (None, diagnostic);
    }
    let positive = f.value.is_empty();
    let literal = if positive { t } else { f };
    let css = literal.value.as_str();
    let Some(decl) = declaration(css, 0, css.len(), &[]) else {
        return (None, diagnostic);
    };
    let Some(_) = decl.directive else {
        return (None, diagnostic);
    };
    if css.trim_end_matches(';').contains(';') {
        return (None, diagnostic);
    }
    let colon = css.find(':').unwrap();
    let prop = css[..colon].trim();
    let value = css[colon + 1..].trim_end_matches(';').trim();
    let tspan = c.consequent.span();
    let fspan = c.alternate.span();
    let testspan = c.test.span();
    let rewritten = format!(
        "{}{}{}{}",
        &expr[testspan.start as usize..tspan.start as usize],
        if positive {
            js_string(value, expr.as_bytes()[tspan.start as usize] as char)
        } else {
            "null".into()
        },
        &expr[tspan.end as usize..fspan.start as usize],
        if positive {
            "null".into()
        } else {
            js_string(value, expr.as_bytes()[fspan.start as usize] as char)
        }
    );
    (Some(format!("style:{prop}={{{rewritten}}}")), diagnostic)
}
fn js_string(value: &str, quote: char) -> String {
    let mut out = String::new();
    out.push(quote);
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c)
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;
    #[test]
    fn css_quotes_and_inline_js_escapes_produce_parseable_directives() {
        let source = r#"<div style='font-family: "Arial";' />"#;
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        let d = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .find(|d| d.rule_name == "svelte/prefer-style-directive")
            .unwrap();
        assert_eq!(
            d.fix.unwrap().replacement,
            "style:font-family=\"&quot;Arial&quot;\""
        );
        let source = r#"<div style={ok ? 'content: "a\'b"' : ''} />"#;
        let parsed = parser::parse_for_lint(source, &allocator);
        let d = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .find(|d| d.rule_name == "svelte/prefer-style-directive")
            .unwrap();
        let fix = d.fix.unwrap();
        assert_eq!(fix.replacement, r#"style:content={ok ? '"a\'b"' : null}"#);
        let output = format!(
            "{}{}{}",
            &source[..fix.span.start as usize],
            fix.replacement,
            &source[fix.span.end as usize..]
        );
        assert!(parser::parse_for_lint(&output, &allocator)
            .errors
            .is_empty());
    }
    #[test]
    fn style_ranges_preserve_css_functions_unicode_and_unsupported_values() {
        let source="<!-- 😀 --><div style=\"background: url('x;y'); color: red!important; width: {size}px\" style:width={width} />";
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/prefer-style-directive")
            .collect();
        assert_eq!(diagnostics.len(), 1);
        let d = &diagnostics[0];
        assert_eq!(
            &source[d.span.start as usize..d.span.end as usize],
            "background: url('x;y');"
        );
        let fix = d.fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "style:background=\"url('x;y')\" style=\"");
        assert!(source.is_char_boundary(fix.span.start as usize));
        assert!(source.is_char_boundary(fix.span.end as usize));
    }
    #[test]
    fn unicode_css_and_braces_in_interpolations_keep_source_boundaries() {
        let source =
            "<!-- 😀 --><div style=\"content: 'é'; color: {color /* } */}; width: 2px\" />";
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/prefer-style-directive")
            .collect();
        assert_eq!(diagnostics.len(), 3);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "content: 'é';"
        );
        assert!(diagnostics
            .iter()
            .all(|d| source.is_char_boundary(d.span.start as usize)
                && source.is_char_boundary(d.span.end as usize)));
        assert!(diagnostics.iter().any(|d| d
            .fix
            .as_ref()
            .unwrap()
            .replacement
            .contains("{color /* } */}")));
    }
}
