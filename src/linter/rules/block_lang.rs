//! `svelte/block-lang` — enforce or disallow specific `lang` attributes on script/style blocks.
//! 💡

use crate::linter::{LintContext, Rule};
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
    ctx.diagnostic(msg, span);
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
            ctx.diagnostic(
                format!(
                    "The <script> block should be present and its lang attribute should be {}.",
                    desc
                ),
                Span::new(1, 1),
            );
        }
        if let Some(allowed) = &script_langs {
            for script in [&ctx.ast.instance, &ctx.ast.module]
                .iter()
                .filter_map(|s| s.as_ref())
            {
                check_block_lang("script", script.span, script.lang.as_deref(), allowed, ctx);
            }
        }

        if enforce_style && ctx.ast.css.is_none() {
            let desc = style_langs
                .as_ref()
                .map_or("omitted".to_string(), |a| pretty_print_langs(a));
            ctx.diagnostic(
                format!(
                    "The <style> block should be present and its lang attribute should be {}.",
                    desc
                ),
                Span::new(1, 1),
            );
        }
        if let Some(allowed) = &style_langs {
            if let Some(style) = &ctx.ast.css {
                check_block_lang("style", style.span, style.lang.as_deref(), allowed, ctx);
            }
        }
    }
}
