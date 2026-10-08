//! Source spans and alternative declaration sets for inline Svelte styles.
use crate::ast::{Attribute, DirectiveKind};
use oxc::ast::ast::Expression;
use oxc::span::{GetSpan, SourceType, Span};

pub(super) type Declaration = (String, Span);

pub(super) fn declarations(attr: &Attribute, source: &str) -> Vec<Vec<Declaration>> {
    match attr {
        Attribute::Directive {
            kind: DirectiveKind::StyleDirective,
            name,
            span,
            ..
        } => {
            vec![vec![(
                name.clone(),
                Span::new(span.start + 6, span.start + 6 + name.len() as u32),
            )]]
        }
        Attribute::NormalAttribute { name, span, .. } if name == "style" => {
            let raw = &source[span.start as usize..span.end as usize];
            let Some(eq) = raw.find('=') else {
                return vec![];
            };
            let mut start = eq + 1;
            while raw
                .as_bytes()
                .get(start)
                .is_some_and(u8::is_ascii_whitespace)
            {
                start += 1;
            }
            let quoted = matches!(raw.as_bytes().get(start), Some(b'\'' | b'"'));
            let end = raw.len() - usize::from(quoted);
            start += usize::from(quoted);
            css_sets(&raw[start..end], span.start + start as u32)
        }
        _ => vec![],
    }
}

// CSS punctuation inside expressions, strings, comments and functions does not
// terminate a declaration. Each standalone interpolation is one alternative set.
fn css_sets(text: &str, base: u32) -> Vec<Vec<Declaration>> {
    let mut sets = vec![];
    let mut start = 0;
    let mut i = 0;
    let mut parentheses = 0;
    while i < text.len() {
        let b = text.as_bytes()[i];
        if text.as_bytes()[i..].starts_with(b"/*") {
            i = text[i + 2..].find("*/").map_or(text.len(), |n| i + n + 4);
            if text[start..i].trim_start().starts_with("/*") {
                start = i;
            }
            continue;
        }
        if b == b'{' {
            let end = expression_end(text, i);
            if text[start..i].trim().is_empty() {
                let mut inline = vec![];
                literal_declarations(
                    &text[i + 1..end.saturating_sub(1)],
                    base + i as u32 + 1,
                    &mut inline,
                );
                if !inline.is_empty() {
                    sets.push(inline);
                }
                start = end;
            }
            i = end;
            continue;
        }
        if matches!(b, b'\'' | b'"') {
            i = quote_end(text, i);
            continue;
        }
        if b == b'(' {
            parentheses += 1;
        }
        if b == b')' {
            parentheses -= 1;
        }
        if b == b';' && parentheses == 0 {
            push_declaration(&text[start..i], base + start as u32, &mut sets);
            start = i + 1;
        }
        i += 1;
    }
    push_declaration(&text[start..], base + start as u32, &mut sets);
    sets
}

fn push_declaration(text: &str, base: u32, sets: &mut Vec<Vec<Declaration>>) {
    let Some(colon) = text.find(':') else { return };
    let key = text[..colon].trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
    {
        return;
    }
    let start = base + (text.len() - text.trim_start().len()) as u32;
    sets.push(vec![(
        key.to_string(),
        Span::new(start, start + key.len() as u32),
    )]);
}

fn literal_declarations(text: &str, base: u32, out: &mut Vec<Declaration>) {
    let allocator = oxc::allocator::Allocator::default();
    if let Ok(expr) =
        oxc::parser::Parser::new(&allocator, text, SourceType::ts()).parse_expression()
    {
        collect_literals(&expr, text, base, out);
    }
}

fn collect_literals(expr: &Expression<'_>, source: &str, base: u32, out: &mut Vec<Declaration>) {
    match expr {
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => {
            let span = expr.span();
            let start = span.start as usize + 1;
            let end = span.end as usize - 1;
            let text = &source[start..end];
            // Template substitutions are values, not independent CSS declarations.
            let mut masked = text.as_bytes().to_vec();
            let mut i = 0;
            while i + 1 < text.len() {
                if text.as_bytes()[i..].starts_with(b"${") {
                    let end = expression_end(text, i + 1);
                    masked[i..end].fill(b' ');
                    i = end;
                } else {
                    i += 1;
                }
            }
            let masked = String::from_utf8(masked).expect("mask preserves UTF-8");
            out.extend(css_sets(&masked, base + start as u32).into_iter().flatten());
        }
        Expression::ConditionalExpression(e) => {
            collect_literals(&e.consequent, source, base, out);
            collect_literals(&e.alternate, source, base, out);
        }
        Expression::LogicalExpression(e) => {
            collect_literals(&e.left, source, base, out);
            collect_literals(&e.right, source, base, out);
        }
        Expression::ParenthesizedExpression(e) => {
            collect_literals(&e.expression, source, base, out)
        }
        _ => {}
    }
}

fn quote_end(text: &str, start: usize) -> usize {
    let quote = text.as_bytes()[start];
    let mut i = start + 1;
    while i < text.len() {
        if text.as_bytes()[i] == b'\\' {
            i += 2;
            continue;
        }
        if text.as_bytes()[i] == quote {
            return i + 1;
        }
        if quote == b'`' && text.as_bytes()[i..].starts_with(b"${") {
            i = expression_end(text, i + 1);
            continue;
        }
        i += 1;
    }
    text.len()
}

pub(super) fn expression_end(text: &str, start: usize) -> usize {
    let allocator = oxc::allocator::Allocator::default();
    if let Ok(expression) =
        oxc::parser::Parser::new(&allocator, &text[start + 1..], SourceType::ts())
            .parse_expression()
    {
        let mut end = start + 1 + expression.span().end as usize;
        loop {
            while text
                .as_bytes()
                .get(end)
                .is_some_and(u8::is_ascii_whitespace)
            {
                end += 1;
            }
            if text.as_bytes()[end..].starts_with(b"/*") {
                end = text[end + 2..]
                    .find("*/")
                    .map_or(text.len(), |n| end + n + 4);
            } else if text.as_bytes()[end..].starts_with(b"//") {
                end = text[end..].find('\n').map_or(text.len(), |n| end + n);
            } else {
                break;
            }
        }
        if text.as_bytes().get(end) == Some(&b'}') {
            return end + 1;
        }
    }
    let mut depth = 1;
    let mut i = start + 1;
    while i < text.len() {
        match text.as_bytes()[i] {
            b'\'' | b'"' | b'`' => {
                i = quote_end(text, i);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternatives_keep_individual_spans_and_css_values_remain_opaque() {
        let text = "color: 'a;b'; {ok ? `color: ${value}` : 'color: blue'}";
        let sets = css_sets(text, 0);
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[1].len(), 2);
        for (name, span) in sets.into_iter().flatten() {
            assert_eq!(name, "color");
            assert_eq!(&text[span.start as usize..span.end as usize], "color");
        }
    }

    #[test]
    fn interpolation_boundaries_follow_js_comments_regex_and_templates() {
        for text in [
            "{ok /* } */ ? 'color:red' : 'color:blue'}",
            "{/}/.test(value) ? 'color:red' : 'color:blue'}",
            "{ok // }\n ? `color:${value /* } */}` : 'color:blue'}",
        ] {
            assert_eq!(expression_end(text, 0), text.len(), "{text}");
            let sets = css_sets(text, 0);
            assert_eq!(sets.len(), 1, "{text}");
            assert_eq!(sets[0].len(), 2, "{text}");
        }
    }
}
