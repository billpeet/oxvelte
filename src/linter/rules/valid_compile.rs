//! Report genuine Svelte compiler warnings and errors.
use crate::linter::{LintContext, Rule};
use oxc::span::Span;

pub struct ValidCompile;
impl Rule for ValidCompile {
    fn name(&self) -> &'static str {
        "svelte/valid-compile"
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if ctx.is_svelte_module
            || !ctx
                .file_path
                .as_ref()
                .is_some_and(|p| p.ends_with(".svelte"))
        {
            return;
        }
        let ignore_warnings = ctx
            .config
            .options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.get("ignoreWarnings"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let global_style = ctx
            .ast
            .css
            .as_ref()
            .filter(|s| {
                has_global_attribute(
                    &ctx.source[s.attrs_span.start as usize..s.attrs_span.end as usize],
                )
            })
            .map(|s| s.span);
        let diagnostics = match ctx.compiler_result() {
            Err(error) => vec![(
                format!("Unable to run Svelte compiler: {error}"),
                Span::new(0, 0),
            )],
            Ok(result) => {
                if ignore_warnings && result.kind == "warn" {
                    return;
                }
                result
                    .warnings
                    .iter()
                    .filter_map(|warning| {
                        if warning.code.as_deref() == Some("missing-declaration") {
                            return None;
                        }
                        if matches!(
                            warning.code.as_deref(),
                            Some("css_unused_selector" | "css-unused-selector")
                        ) && global_style.zip(warning.span).is_some_and(|(style, span)| {
                            style.start <= span.start && span.end <= style.end
                        }) {
                            return None;
                        }
                        let transformed = if result.kind == "warn" {
                            if warning.filtered {
                                return None;
                            }
                            warning.report.as_ref()
                        } else {
                            None
                        };
                        let (message, code, span) = if let Some(w) = transformed {
                            (&w.message, w.code.as_deref(), w.span)
                        } else {
                            (&warning.message, warning.code.as_deref(), warning.span)
                        };
                        Some((
                            format!(
                                "{message}{}",
                                code.map(|c| format!("({c})")).unwrap_or_default()
                            ),
                            span.unwrap_or_else(|| Span::new(0, 0)),
                        ))
                    })
                    .collect()
            }
        };
        for (message, span) in diagnostics {
            ctx.diagnostic(message, span);
        }
    }
}

/// Attribute names are read outside quoted values and expression braces.
fn has_global_attribute(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && !matches!(bytes[i], b'=' | b'>' | b'/')
        {
            i += 1;
        }
        if i == start {
            i += 1;
            continue;
        }
        if &source[start..i] == "global" {
            return true;
        }
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        match bytes.get(i).copied() {
            Some(quote @ (b'\'' | b'"')) => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            Some(b'{') => {
                let mut depth = 0;
                let mut quote = None;
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if let Some(q) = quote {
                        if b == b'\\' {
                            i += 1;
                        } else if b == q {
                            quote = None;
                        }
                    } else if matches!(b, b'\'' | b'"' | b'`') {
                        quote = Some(b);
                    } else if b == b'{' {
                        depth += 1;
                    } else if b == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
            }
            _ => {
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
            }
        }
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_attribute_names_do_not_match_values() {
        assert!(has_global_attribute(" lang='scss' global "));
        assert!(has_global_attribute(" lang='css' global={true} "));
        assert!(!has_global_attribute(" note=' global ' class=global "));
        assert!(!has_global_attribute(
            " note={ x > 2 ? 'global' : '' } data-global "
        ));
    }
}
