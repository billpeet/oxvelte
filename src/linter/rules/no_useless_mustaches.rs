//! `svelte/no-useless-mustaches` — disallow unnecessary mustache interpolations.
//! ⭐ Recommended, 🔧 Fixable
//!
use crate::ast::{
    Attribute, AttributeMeta, AttributeQuote, AttributeValue, AttributeValuePart, DirectiveKind,
    TemplateNode,
};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use crate::parser::expression::{parse_template_expression, unwrap_template_expression};
use oxc::allocator::Allocator;
use oxc::ast::ast::Expression;
use oxc::span::Span;

pub struct NoUselessMustaches;

impl Rule for NoUselessMustaches {
    fn name(&self) -> &'static str {
        "svelte/no-useless-mustaches"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn is_fixable(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let opts = ctx
            .config
            .options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first());
        let get_bool = |key| {
            opts.and_then(|v| v.get(key))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        let ignore_comment = get_bool("ignoreIncludesComment");
        let ignore_escape = get_bool("ignoreStringEscape");

        walk_template_nodes(&ctx.ast.html, &mut |node| {
            if let TemplateNode::MustacheTag(tag) = node {
                check_mustache_tag(tag, ctx, ignore_comment, ignore_escape);
            }
            if let TemplateNode::Element(el) = node {
                for (attr, meta) in el.attributes.iter().zip(&el.attribute_meta) {
                    match attr {
                        Attribute::NormalAttribute { value, name, .. } => {
                            // `this={…}` on a `<svelte:component>` / `<svelte:element>` /
                            // similar carries semantic meaning — don't suggest collapsing.
                            if name == "this" && el.kind().is_svelte_special() {
                                continue;
                            }
                            check_attribute_value(value, meta, ctx, ignore_comment, ignore_escape);
                        }
                        Attribute::Directive {
                            kind: DirectiveKind::StyleDirective,
                            value,
                            ..
                        } => {
                            check_attribute_value(value, meta, ctx, ignore_comment, ignore_escape);
                        }
                        _ => {}
                    }
                }
            }
        });
    }
}

fn check_mustache_tag<'a>(
    tag: &crate::ast::MustacheTag<'a>,
    ctx: &mut LintContext<'_>,
    ignore_comment: bool,
    ignore_escape: bool,
) {
    check_expression(
        &tag.expression,
        tag.span,
        None,
        ctx,
        ignore_comment,
        ignore_escape,
    );
}

fn check_attribute_value(
    value: &AttributeValue,
    meta: &AttributeMeta<'_>,
    ctx: &mut LintContext<'_>,
    ignore_comment: bool,
    ignore_escape: bool,
) {
    match value {
        AttributeValue::Expression(expr) => {
            if let Some(span) = meta.mustache_span {
                check_expression(expr, span, Some(meta), ctx, ignore_comment, ignore_escape);
            }
        }
        AttributeValue::Concat(parts) => {
            for (part, part_meta) in parts.iter().zip(&meta.parts) {
                if let (AttributeValuePart::Expression(expr), Some(span)) =
                    (part, part_meta.mustache_span)
                {
                    check_expression(expr, span, Some(meta), ctx, ignore_comment, ignore_escape);
                }
            }
        }
        _ => {}
    }
}

fn check_expression(
    expr_text: &str,
    span: Span,
    attribute: Option<&AttributeMeta<'_>>,
    ctx: &mut LintContext<'_>,
    ignore_comment: bool,
    ignore_escape: bool,
) {
    let alloc = Allocator::default();
    let result = parse_template_expression(expr_text, &alloc);
    if !result.errors.is_empty() {
        return;
    }
    let Some(expr) = unwrap_template_expression(&result) else {
        return;
    };
    let Some(raw) = trivial_string_raw(expr) else {
        return;
    };
    let has_comment = !result.program.comments.is_empty();
    let has_escape = has_useful_escape(raw);
    if (ignore_comment && has_comment)
        || (ignore_escape && has_escape)
        || raw.contains('{')
        || (is_template_literal(expr) && raw.contains(['\n', '\r']))
    {
        return;
    }
    let message = "Unexpected mustache interpolation with a string literal value.";
    if has_comment
        || has_escape
        || raw.contains('\n')
        || raw.starts_with(char::is_whitespace)
        || raw.ends_with(char::is_whitespace)
    {
        ctx.diagnostic(message, span);
        return;
    }
    let mut chars = raw.chars();
    let mut unescaped = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                unescaped.push(next);
            }
        } else {
            unescaped.push(ch);
        }
    }
    let fix = if let Some(meta) = attribute {
        match meta.quote {
            Some(AttributeQuote::Double) => Fix {
                span,
                replacement: unescaped.replace('"', "&quot;"),
            },
            Some(AttributeQuote::Single) => Fix {
                span,
                replacement: unescaped.replace('\'', "&apos;"),
            },
            _ => {
                // Normalize quote insertions and this replacement into one edit,
                // preserving other value parts between them.
                let full = meta.value_full_span.unwrap_or(span);
                let replacement = format!(
                    "\"{}{}{}\"",
                    &ctx.source[full.start as usize..span.start as usize],
                    unescaped.replace('"', "&quot;"),
                    &ctx.source[span.end as usize..full.end as usize]
                );
                Fix {
                    span: full,
                    replacement,
                }
            }
        }
    } else {
        Fix {
            span,
            replacement: unescaped.replace('<', "&lt;").replace('>', "&gt;"),
        }
    };
    ctx.diagnostic_with_fix(message, span, fix);
}

/// Match vendor's type guard: return the between-quotes raw string when
/// the expression is either a string `Literal` or a `TemplateLiteral` with
/// zero interpolations. Other expression shapes carry meaning — don't flag.
fn trivial_string_raw<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(lit) => {
            let raw_with_quotes = lit.raw.as_ref().map(|a| a.as_str()).unwrap_or("");
            if raw_with_quotes.len() < 2 {
                return None;
            }
            Some(&raw_with_quotes[1..raw_with_quotes.len() - 1])
        }
        Expression::TemplateLiteral(tl) => {
            if !tl.expressions.is_empty() {
                return None;
            }
            let quasi = tl.quasis.first()?;
            Some(quasi.value.raw.as_str())
        }
        _ => None,
    }
}

fn is_template_literal(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::TemplateLiteral(_))
}

/// Vendor `no-useless-mustaches.ts:91–114`: an escape is "useful" iff it
/// maps to a different cooked character — one of `\n \r \v \t \b \f \u \x`.
fn has_useful_escape(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if matches!(
                bytes[i + 1],
                b'n' | b'r' | b'v' | b't' | b'b' | b'f' | b'u' | b'x'
            ) {
                return true;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
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
            .filter(|d| d.rule_name == "svelte/no-useless-mustaches")
            .collect()
    }

    #[test]
    fn parity_regression_preserves_source_boundaries() {
        let source = "<!-- 😀 --><div title=\"prefix {'a'} {'b'}\" />{'<b>'}{ /* keep */ 'comment' }{'space '}";
        let diagnostics = lint(source, serde_json::json!([]));
        assert_eq!(diagnostics.len(), 5);
        assert_eq!(diagnostics[0].fix.as_ref().unwrap().replacement, "a");
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "{'a'}"
        );
        assert_eq!(
            diagnostics[2].fix.as_ref().unwrap().replacement,
            "&lt;b&gt;"
        );
        assert!(diagnostics[3].fix.is_none());
        assert!(diagnostics[4].fix.is_none());
    }
}
