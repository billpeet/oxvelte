//! `svelte/block-lang` — enforce or disallow specific `lang` attributes on script/style blocks.
//! 💡

use crate::linter::{Fix, LintContext, Rule, Suggestion};
use oxc::span::Span;

pub struct BlockLang;

fn pretty_print_langs(allowed: &[Option<String>]) -> String {
    let has_null = allowed.iter().any(|a| a.is_none());
    let named: Vec<&str> = allowed.iter().filter_map(|a| a.as_deref()).collect();

    match (has_null, named.len()) {
        (true, 0) => "omitted".to_string(),
        (true, 1) => format!("either omitted or \"{}\"", named[0]),
        (true, _) => {
            let quoted: Vec<String> = named.iter().map(|s| format!("\"{}\"", s)).collect();
            format!("either omitted or one of {}", quoted.join(", "))
        }
        (false, 1) => format!("\"{}\"", named[0]),
        (false, _) => {
            let quoted: Vec<String> = named.iter().map(|s| format!("\"{}\"", s)).collect();
            format!("one of {}", quoted.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{linter::RuleConfig, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn language_defaults_options_and_reports_match_upstream() {
        for (source, options, expected) in [
            (
                "<script lang='ts'></script>",
                serde_json::json!([{}]),
                "omitted",
            ),
            (
                "<script lang='ts'></script>",
                serde_json::json!([{ "script": "TS" }]),
                "\"TS\"",
            ),
            (
                "<script></script>",
                serde_json::json!([{ "script": ["ts", "typescript"] }]),
                "one of \"ts\", \"typescript\"",
            ),
        ] {
            let allocator = Allocator::default();
            let parsed = parser::parse(source, &allocator);
            let mut ctx = LintContext::with_config(
                &parsed.ast,
                source,
                RuleConfig {
                    options: Some(options),
                    settings: None,
                },
            );
            BlockLang.run(&mut ctx);
            assert_eq!(ctx.diagnostics.len(), 1);
            assert_eq!(
                ctx.diagnostics[0].message,
                format!("The lang attribute of the <script> block should be {expected}.")
            );
            assert!(ctx.diagnostics[0].fix.is_none());
        }
    }

    #[test]
    fn missing_blocks_report_second_column() {
        let source = "<p>Hello</p>";
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        let mut ctx = LintContext::with_config(
            &parsed.ast,
            source,
            RuleConfig {
                options: Some(
                    serde_json::json!([{ "enforceScriptPresent": true, "enforceStylePresent": true }]),
                ),
                settings: None,
            },
        );
        BlockLang.run(&mut ctx);
        assert_eq!(ctx.diagnostics.len(), 2);
        assert!(ctx
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.span == Span::new(1, 1) && diagnostic.fix.is_none()));
    }

    fn apply(source: &str, suggestion: &Suggestion) -> String {
        format!(
            "{}{}{}",
            &source[..suggestion.fix.span.start as usize],
            suggestion.fix.replacement,
            &source[suggestion.fix.span.end as usize..]
        )
    }

    #[test]
    fn suggestions_replace_only_the_attribute_and_preserve_alternative_order() {
        let source =
            "<script data-note=\"lang='fake'\"\n lang = 'js' context='module'>let x = 1;</script>";
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        let script = parsed.ast.module.as_ref().unwrap();
        let suggestions = replacement_suggestions(
            "script",
            script.span,
            script.attrs_span,
            &[
                None,
                Some("typescript".into()),
                Some("ts".into()),
                Some(String::new()),
            ],
            source,
        );
        assert_eq!(suggestions.len(), 2);
        assert_eq!(
            suggestions[0].description,
            "Replace a <script> block with the lang attribute set to \"typescript\"."
        );
        assert_eq!(
            apply(source, &suggestions[0]),
            source.replace("lang = 'js'", "lang=\"typescript\"")
        );
        assert_eq!(
            apply(source, &suggestions[1]),
            source.replace("lang = 'js'", "lang=\"ts\"")
        );
        let omitted =
            replacement_suggestions("script", script.span, script.attrs_span, &[None], source);
        assert_eq!(
            apply(source, &omitted[0]),
            source.replace(" lang = 'js'", "")
        );
    }

    #[test]
    fn missing_and_existing_blocks_have_distinct_insertions() {
        let source = "<style media='screen'></style>";
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        let style = parsed.ast.css.as_ref().unwrap();
        let allowed = [None, Some(String::new()), Some("scss".into())];
        let existing =
            replacement_suggestions("style", style.span, style.attrs_span, &allowed, source);
        assert_eq!(existing.len(), 1);
        assert_eq!(
            existing[0].description,
            "Add lang attribute to a <style> block with the value \"scss\"."
        );
        assert_eq!(
            apply(source, &existing[0]),
            "<style lang=\"scss\" media='screen'></style>"
        );
        let script = missing_block_suggestions("script", &allowed, source);
        assert_eq!(
            apply(source, &script[0]),
            format!("<script lang=\"scss\">\n</script>\n\n{source}")
        );
        let style = missing_block_suggestions("style", &allowed, source);
        assert_eq!(
            apply(source, &style[0]),
            format!("{source}<style lang=\"scss\">\n</style>\n\n")
        );
    }

    #[test]
    fn missing_block_reports_and_suggestions_use_valid_utf8_boundaries() {
        for source in ["", "é", "😀"] {
            let allocator = Allocator::default();
            let parsed = parser::parse(source, &allocator);
            let mut ctx = LintContext::with_config(
                &parsed.ast,
                source,
                RuleConfig {
                    options: Some(serde_json::json!([{
                        "enforceScriptPresent": true, "enforceStylePresent": true,
                        "script": "ts", "style": "scss"
                    }])),
                    settings: None,
                },
            );
            BlockLang.run(&mut ctx);
            assert_eq!(ctx.diagnostics.len(), 2);
            for diagnostic in &ctx.diagnostics {
                assert_eq!(diagnostic.span.start, diagnostic.span.end);
                assert!(source.is_char_boundary(diagnostic.span.start as usize));
                assert!(diagnostic.span.end as usize <= source.len());
                assert_eq!(diagnostic.suggestions.len(), 1);
                for suggestion in &diagnostic.suggestions {
                    assert!(source.is_char_boundary(suggestion.fix.span.start as usize));
                    assert!(source.is_char_boundary(suggestion.fix.span.end as usize));
                    assert!(suggestion.fix.span.end as usize <= source.len());
                    assert!(apply(source, suggestion).contains(source));
                }
            }
            assert_eq!(ctx.diagnostics[0].span.start as usize, source.len());
        }
    }
}

/// Upstream reports a synthetic column 2, even when it lies outside the source.
/// Keep byte spans valid: empty source uses column 1; an initial astral character
/// uses the next UTF-8 boundary, which represents UTF-16 column 3 instead of 2.
fn missing_block_span(source: &str) -> Span {
    let position = source.chars().next().map_or(0, char::len_utf8) as u32;
    Span::new(position, position)
}

fn parse_langs(opts: Option<&serde_json::Value>, key: &str) -> Option<Vec<Option<String>>> {
    opts.and_then(|o| o.get(key)).and_then(|v| {
        if let Some(arr) = v.as_array() {
            Some(arr.iter().map(|v| v.as_str().map(String::from)).collect())
        } else {
            Some(vec![v.as_str().map(String::from)])
        }
    })
}

fn check_block_lang(
    tag: &str,
    span: Span,
    attrs_span: Span,
    block_lang: Option<&str>,
    allowed: &[Option<String>],
    ctx: &mut LintContext<'_>,
) {
    let lang = block_lang.map(|l| l.to_lowercase());
    let lang_ref = lang.as_deref();
    if allowed.iter().any(|a| a.as_deref() == lang_ref) {
        return;
    }

    let msg = format!(
        "The lang attribute of the <{}> block should be {}.",
        tag,
        pretty_print_langs(allowed)
    );
    ctx.diagnostic_with_suggestions(
        msg,
        span,
        replacement_suggestions(tag, span, attrs_span, allowed, ctx.source),
    );
}

/// Find the whole attribute token without matching `lang` inside another value.
fn lang_attribute_span(source: &str, attrs_span: Span) -> Option<Span> {
    let bytes = source.as_bytes();
    let end = attrs_span.end as usize;
    let mut pos = attrs_span.start as usize;
    while pos < end {
        while pos < end && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let start = pos;
        while pos < end
            && !bytes[pos].is_ascii_whitespace()
            && !matches!(bytes[pos], b'=' | b'>' | b'/')
        {
            pos += 1;
        }
        if start == pos {
            pos += 1;
            continue;
        }
        let name_end = pos;
        let mut attribute_end = pos;
        while pos < end && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos < end && bytes[pos] == b'=' {
            pos += 1;
            while pos < end && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let mut quote = None;
            let mut brace_depth = 0_u32;
            while pos < end {
                let byte = bytes[pos];
                if let Some(delimiter) = quote {
                    if byte == delimiter {
                        quote = None;
                    } else if byte == b'\\' {
                        pos = (pos + 1).min(end);
                    }
                } else if matches!(byte, b'\'' | b'"' | b'`') {
                    quote = Some(byte);
                } else if byte == b'{' {
                    brace_depth += 1;
                } else if byte == b'}' {
                    brace_depth = brace_depth.saturating_sub(1);
                } else if brace_depth == 0
                    && (byte.is_ascii_whitespace() || matches!(byte, b'>' | b'/'))
                {
                    break;
                }
                pos += 1;
            }
            attribute_end = pos;
        }
        if &source[start..name_end] == "lang" {
            return Some(Span::new(start as u32, attribute_end as u32));
        }
    }
    None
}

fn replacement_suggestions(
    tag: &str,
    block_span: Span,
    attrs_span: Span,
    allowed: &[Option<String>],
    source: &str,
) -> Vec<Suggestion> {
    let attribute = lang_attribute_span(source, attrs_span);
    let named: Vec<_> = allowed
        .iter()
        .filter_map(|lang| lang.as_deref())
        .filter(|lang| !lang.is_empty())
        .collect();
    if named.is_empty() && allowed.iter().any(Option::is_none) {
        if let Some(attribute) = attribute {
            let preceding = source[..attribute.start as usize]
                .chars()
                .next_back()
                .map_or(0, char::len_utf8);
            return vec![Suggestion {
                description: format!("Replace a <{tag}> block with the lang attribute omitted."),
                fix: Fix {
                    span: Span::new(attribute.start - preceding as u32, attribute.end),
                    replacement: String::new(),
                },
            }];
        }
    }
    named
        .into_iter()
        .map(|lang| {
            if let Some(attribute) = attribute {
                Suggestion {
                    description: format!(
                        "Replace a <{tag}> block with the lang attribute set to \"{lang}\"."
                    ),
                    fix: Fix {
                        span: attribute,
                        replacement: format!("lang=\"{lang}\""),
                    },
                }
            } else {
                let position = block_span.start + tag.len() as u32 + 1;
                Suggestion {
                    description: format!(
                        "Add lang attribute to a <{tag}> block with the value \"{lang}\"."
                    ),
                    fix: Fix {
                        span: Span::new(position, position),
                        replacement: format!(" lang=\"{lang}\""),
                    },
                }
            }
        })
        .collect()
}

fn missing_block_suggestions(
    tag: &str,
    allowed: &[Option<String>],
    source: &str,
) -> Vec<Suggestion> {
    let position = if tag == "script" {
        0
    } else {
        source.len() as u32
    };
    allowed
        .iter()
        .filter_map(|lang| lang.as_deref())
        .filter(|lang| !lang.is_empty())
        .map(|lang| Suggestion {
            description: format!(
                "Add a lang attribute to a <{tag}> block with the value \"{lang}\"."
            ),
            fix: Fix {
                span: Span::new(position, position),
                replacement: format!("<{tag} lang=\"{lang}\">\n</{tag}>\n\n"),
            },
        })
        .collect()
}

impl Rule for BlockLang {
    fn name(&self) -> &'static str {
        "svelte/block-lang"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let opts = ctx
            .config
            .options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first());

        let script_langs = Some(parse_langs(opts, "script").unwrap_or_else(|| vec![None]));
        let style_langs = Some(parse_langs(opts, "style").unwrap_or_else(|| vec![None]));
        let enforce_script = opts
            .and_then(|o| o.get("enforceScriptPresent"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let enforce_style = opts
            .and_then(|o| o.get("enforceStylePresent"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if enforce_script && ctx.ast.instance.is_none() && ctx.ast.module.is_none() {
            let desc = script_langs
                .as_ref()
                .map_or("omitted".to_string(), |a| pretty_print_langs(a));
            ctx.diagnostic_with_suggestions(
                format!(
                    "The <script> block should be present and its lang attribute should be {}.",
                    desc
                ),
                missing_block_span(ctx.source),
                missing_block_suggestions("script", script_langs.as_ref().unwrap(), ctx.source),
            );
        }
        if let Some(allowed) = &script_langs {
            for script in [&ctx.ast.instance, &ctx.ast.module]
                .iter()
                .filter_map(|s| s.as_ref())
            {
                check_block_lang(
                    "script",
                    script.span,
                    script.attrs_span,
                    script.lang.as_deref(),
                    allowed,
                    ctx,
                );
            }
        }

        if enforce_style && ctx.ast.css.is_none() {
            let desc = style_langs
                .as_ref()
                .map_or("omitted".to_string(), |a| pretty_print_langs(a));
            ctx.diagnostic_with_suggestions(
                format!(
                    "The <style> block should be present and its lang attribute should be {}.",
                    desc
                ),
                missing_block_span(ctx.source),
                missing_block_suggestions("style", style_langs.as_ref().unwrap(), ctx.source),
            );
        }
        if let Some(allowed) = &style_langs {
            if let Some(style) = &ctx.ast.css {
                check_block_lang(
                    "style",
                    style.span,
                    style.attrs_span,
                    style.lang.as_deref(),
                    allowed,
                    ctx,
                );
            }
        }
    }
}
