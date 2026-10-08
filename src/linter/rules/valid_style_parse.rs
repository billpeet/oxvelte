//! `svelte/valid-style-parse` — report style parsing errors in `<style>` blocks.

use crate::linter::{LintContext, Rule};
use crate::parser::css::{parse_css, CssParseErrorKind};

pub struct ValidStyleParse;

const SUPPORTED_STYLE_LANGS: &[&str] = &["css", "scss", "less", "postcss", "stylus", "sass"];

impl Rule for ValidStyleParse {
    fn name(&self) -> &'static str {
        "svelte/valid-style-parse"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let Some(style) = &ctx.ast.css else { return };
        if style.content.trim().is_empty() {
            return;
        }
        if let Some(lang) = &style.lang {
            if !SUPPORTED_STYLE_LANGS.contains(&lang.as_str()) {
                ctx.diagnostic(
                    format!("Found unsupported style element language \"{}\"", lang),
                    style.span,
                );
                return;
            }
        }
        // Upstream reports parser-service style context failures. Until this
        // project has dedicated preprocessors for every style lang, the
        // Svelte-compatible CSS parser is the canonical syntax check here.
        let cs = style.content_span.start;
        let parsed = parse_css(&style.content, cs);
        let err_pos = if let Some(error) = parsed.errors.first() {
            Some(error.position as u32)
        } else if !parsed.error_positions.is_empty() {
            Some(parsed.error_positions[0] as u32)
        } else if !style.content[parsed.position..].trim().is_empty() {
            Some(parsed.position as u32)
        } else {
            None
        };
        if let Some(ep) = err_pos {
            let (position, reason) = style_error_message(
                &style.content,
                ep as usize,
                parsed.errors.first().map(|e| e.kind),
            );
            let prefix = &style.content[..position];
            let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
            let column = prefix
                .rsplit('\n')
                .next()
                .unwrap_or(prefix)
                .encode_utf16()
                .count()
                + 1;
            let filename = ctx
                .file_path
                .as_deref()
                .unwrap_or("<input>")
                .replace('\\', "/");
            let cwd = std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/") + "/");
            let filename = cwd
                .as_deref()
                .and_then(|cwd| filename.strip_prefix(cwd))
                .unwrap_or(&filename);
            ctx.diagnostic(
                format!("Error parsing style element. Error message: \"{filename}:{line}:{column}: {reason}\""),
                style.span,
            );
        }
    }
}

fn style_error_message(
    css: &str,
    position: usize,
    kind: Option<CssParseErrorKind>,
) -> (usize, String) {
    if kind == Some(CssParseErrorKind::UnclosedComment) {
        return (position, "Unclosed comment".into());
    }
    if kind == Some(CssParseErrorKind::InvalidDeclaration) {
        let tail = &css[position..];
        let property_len = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let after_property = &tail[property_len..];
        let whitespace_len = after_property.len() - after_property.trim_start().len();
        let next = property_len + whitespace_len;
        let word_position =
            if next < tail.len() && !matches!(tail.as_bytes()[next], b':' | b';' | b'}') {
                position + next
            } else {
                position
            };
        let word = css[word_position..]
            .split(|c: char| c.is_whitespace() || matches!(c, ';' | '}' | ':'))
            .next()
            .unwrap_or("");
        return (word_position, format!("Unknown word {word}"));
    }
    (position, "Unclosed block".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linter::LintContext;
    use crate::parser;
    use oxc::allocator::Allocator;

    fn diagnostics_for(source: &str) -> Vec<String> {
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        let mut ctx = LintContext::new(&parsed.ast, source);
        ValidStyleParse.run(&mut ctx);
        ctx.into_diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    #[test]
    fn accepts_common_scss_syntax_without_preprocessor() {
        let messages = diagnostics_for(
            r#"<style lang="scss">
                $brand: red;
                %button { color: $brand; }
                .button { @extend %button; }
            </style>"#,
        );

        assert!(messages.is_empty(), "{messages:?}");
    }

    #[test]
    fn reports_unknown_style_language_before_parsing() {
        let messages = diagnostics_for(r#"<style lang="wat">.x { color red; }</style>"#);
        assert_eq!(
            messages,
            vec!["Found unsupported style element language \"wat\"".to_string()]
        );
    }

    #[test]
    fn reports_css_error_with_real_filename_and_style_relative_location() {
        let source = "<!-- é -->\n<style data-label='>'>.x { color red; }</style>";
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        let mut ctx = LintContext::new(&parsed.ast, source);
        ctx.file_path = Some("src/Panel.svelte".into());
        ValidStyleParse.run(&mut ctx);
        let diagnostics = ctx.into_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "Error parsing style element. Error message: \"src/Panel.svelte:1:12: Unknown word red\"");
        assert_eq!(
            diagnostics[0].span.start,
            source.find("<style").unwrap() as u32
        );
    }
}
