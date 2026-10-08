use crate::ast::TemplateNode;
use crate::compiler::IgnoreItem;
use crate::linter::{walk_template_nodes, LintContext};
use oxc::span::GetSpan;
use oxc::span::Span;

/// Apply parser-style leading comments to compiler warnings. Compiler-native
/// suppression is intentionally disabled before this step.
pub(crate) fn resolve(ctx: &LintContext<'_>, result: &mut crate::compiler::CompileResult) {
    result.unused_ignores = result
        .ignore_items
        .iter()
        .filter(|item| item.code.is_some())
        .cloned()
        .collect();
    if result.kind == "error" {
        return;
    }
    let mut targets: Vec<(Span, bool)> = Vec::new();
    let mut comments = Vec::new();
    walk_template_nodes(&ctx.ast.html, &mut |node| match node {
        TemplateNode::Comment(node) => comments.push(node.span),
        TemplateNode::Element(node) => targets.push((node.span, true)),
        TemplateNode::IfBlock(node) => targets.push((node.span, false)),
        TemplateNode::EachBlock(node) => targets.push((node.span, false)),
        TemplateNode::AwaitBlock(node) => targets.push((node.span, false)),
        TemplateNode::KeyBlock(node) => targets.push((node.span, false)),
        _ => {}
    });
    if let Some(style) = &ctx.ast.css {
        targets.push((style.span, true));
    }
    for (semantic, offset) in [
        (ctx.instance_semantic, ctx.instance_content_offset),
        (ctx.module_semantic, ctx.module_content_offset),
    ] {
        let Some(semantic) = semantic else { continue };
        for comment in &semantic.nodes().program().comments {
            comments.push(Span::new(
                offset + comment.span.start,
                offset + comment.span.end,
            ));
        }
        for statement in &semantic.nodes().program().body {
            let span = statement.span();
            targets.push((Span::new(offset + span.start, offset + span.end), true));
        }
    }
    comments.sort_by_key(|span| span.start);
    result.warnings.retain(|warning| {
        let Some(code) = warning.code.as_deref() else {
            return true;
        };
        let Some(span) = warning.span else {
            return true;
        };
        let mut containing: Vec<_> = targets
            .iter()
            .filter(|(target, _)| target.start <= span.start && span.start < target.end)
            .copied()
            .collect();
        containing.sort_by_key(|(span, _)| span.end - span.start);
        let Some(first) = containing
            .iter()
            .position(|(_, warning_target)| *warning_target)
        else {
            return true;
        };
        let mut used = false;
        for (target, _) in &containing[first..] {
            used |= consume_ignore(
                ctx.source,
                *target,
                &comments,
                &result.ignore_items,
                &mut result.unused_ignores,
                |item| {
                    item.code.as_deref() == Some(code) || item.code_for_v5.as_deref() == Some(code)
                },
            );
        }
        !used
    });
    // A style body discarded because its preprocessor is unavailable cannot
    // produce CSS warnings; upstream treats the corresponding ignore as used.
    for style in &result.strip_style_elements {
        consume_ignore(
            ctx.source,
            *style,
            &comments,
            &result.ignore_items,
            &mut result.unused_ignores,
            |item| {
                [item.code.as_deref(), item.code_for_v5.as_deref()]
                    .into_iter()
                    .flatten()
                    .any(|code| {
                        matches!(
                            code,
                            "css-unused-selector"
                                | "css_unused_selector"
                                | "css-invalid-global"
                                | "css-invalid-global-selector"
                        )
                    })
            },
        );
    }
}

fn consume_ignore(
    source: &str,
    target: Span,
    comments: &[Span],
    all: &[IgnoreItem],
    unused: &mut Vec<IgnoreItem>,
    matches: impl Fn(&IgnoreItem) -> bool,
) -> bool {
    let mut end = target.start;
    for comment in comments
        .iter()
        .rev()
        .filter(|comment| comment.end <= target.start)
    {
        let between = &source[comment.end as usize..end as usize];
        // HTMLText and opening parentheses are excluded by the upstream token
        // filter. Other markup/control tokens terminate the leading comments.
        let markup = source[target.start as usize..].starts_with(['<', '{']);
        let no_significant_token = if markup {
            crate::parser::scanner::SvelteScanner::new(between)
                .all(|token| matches!(token.kind, crate::parser::scanner::TokenKind::Text(_)))
        } else {
            between.chars().all(|c| js_whitespace(c) || c == '(')
        };
        if !no_significant_token {
            break;
        }
        end = comment.start;
        if let Some(item) = all
            .iter()
            .find(|item| item.token_span == *comment && matches(item))
        {
            unused.retain(|other| other.span != item.span);
            return true;
        }
    }
    false
}
pub(crate) fn items(ctx: &LintContext<'_>) -> Vec<IgnoreItem> {
    let mut items = Vec::new();
    walk_template_nodes(&ctx.ast.html, &mut |node| {
        if let TemplateNode::Comment(comment) = node {
            parse_comment(
                &comment.data,
                comment.span.start + 4,
                comment.span,
                &mut items,
            );
        }
    });
    for (semantic, offset) in [
        (ctx.instance_semantic, ctx.instance_content_offset),
        (ctx.module_semantic, ctx.module_content_offset),
    ] {
        let Some(semantic) = semantic else { continue };
        for comment in &semantic.nodes().program().comments {
            let text =
                &semantic.source_text()[comment.span.start as usize..comment.span.end as usize];
            let body = if comment.is_line() {
                &text[2..]
            } else {
                &text[2..text.len() - 2]
            };
            let span = Span::new(offset + comment.span.start, offset + comment.span.end);
            parse_comment(body, span.start + 2, span, &mut items);
        }
    }
    items.sort_by_key(|item| std::cmp::Reverse(item.span.start));
    items
}

fn parse_comment(body: &str, body_start: u32, token_span: Span, items: &mut Vec<IgnoreItem>) {
    let trimmed = body.trim_start_matches(js_whitespace);
    let Some(rest) = trimmed.strip_prefix("svelte-ignore") else {
        return;
    };
    // Upstream requires whitespace after the directive, including for missing codes.
    if !rest.chars().next().is_some_and(js_whitespace) {
        return;
    }
    let codes = rest.trim_start_matches(js_whitespace);
    let codes_start = body_start + (body.len() - codes.len()) as u32;
    // Parenthetical explanations occupy their original bytes to retain locations.
    static NOTES: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let notes = NOTES.get_or_init(|| regex::Regex::new(r"\([^)]*\)").unwrap());
    let processed = notes.replace_all(codes, |captures: &regex::Captures<'_>| {
        " ".repeat(captures[0].len())
    });
    if processed.trim_matches(js_whitespace).is_empty() {
        items.push(IgnoreItem {
            code: None,
            code_for_v5: None,
            span: token_span,
            token_span,
        });
        return;
    }
    // Match the pinned helper's separator iteration, including its handling of
    // a trailing code without a separator.
    static SEPARATOR: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let separator = SEPARATOR
        .get_or_init(|| regex::Regex::new(r"[\s\x{FEFF}]*[\s\x{FEFF},][\s\x{FEFF}]*").unwrap());
    let mut last_end = 0;
    let initial_count = items.len();
    for matched in separator.find_iter(&processed) {
        if matched.start() > last_end {
            push_code(
                &processed[last_end..matched.start()],
                codes_start + last_end as u32,
                token_span,
                items,
            );
        }
        last_end = matched.end();
    }
    if items.len() == initial_count {
        push_code(&processed, codes_start, token_span, items);
    }
}

fn js_whitespace(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn push_code(code: &str, start: u32, token_span: Span, items: &mut Vec<IgnoreItem>) {
    let replacement = match code {
        "non-top-level-reactive-declaration" => "reactive_declaration_invalid_placement",
        "module-script-reactive-declaration" => "reactive_declaration_module_script",
        "empty-block" => "block_empty",
        "avoid-is" => "attribute_avoid_is",
        "invalid-html-attribute" => "attribute_invalid_property_name",
        "a11y-structure" => "a11y_figcaption_parent",
        "illegal-attribute-character" => "attribute_illegal_colon",
        "invalid-rest-eachblock-binding" => "bind_invalid_each_rest",
        "unused-export-let" => "export_let_unused",
        _ => code,
    };
    items.push(IgnoreItem {
        code: Some(code.to_owned()),
        code_for_v5: Some(replacement.replace('-', "_")),
        span: Span::new(start, start + code.len() as u32),
        token_span,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_positions_preserve_unicode_and_parenthetical_notes() {
        let body = "\u{feff} svelte-ignore a11y-autofocus (café 🦀), unused-export-let ";
        let mut items = Vec::new();
        parse_comment(body, 14, Span::new(10, 100), &mut items);
        assert_eq!(items.len(), 2);
        for item in &items {
            assert_eq!(
                &body[(item.span.start - 14) as usize..(item.span.end - 14) as usize],
                item.code.as_deref().unwrap()
            );
        }
        assert_eq!(items[1].code_for_v5.as_deref(), Some("export_let_unused"));
    }

    #[test]
    fn explanations_without_codes_report_the_comment() {
        let mut items = Vec::new();
        let token = Span::new(7, 50);
        parse_comment(" svelte-ignore (explanation) ", 11, token, &mut items);
        assert_eq!(items.len(), 1);
        assert!(items[0].code.is_none());
        assert_eq!(items[0].span, token);
        items.clear();
        parse_comment(" svelte-ignore-extra ", 11, token, &mut items);
        parse_comment(" svelte-ignore", 11, token, &mut items);
        assert!(items.is_empty());
    }

    #[test]
    fn leading_comments_stop_at_template_control_boundaries() {
        let source = "<!-- svelte-ignore foo -->\n{:then value}\n<img>";
        let comment = Span::new(0, 26);
        let start = source.find("<img>").unwrap() as u32;
        let mut items = Vec::new();
        parse_comment(" svelte-ignore foo ", 4, comment, &mut items);
        let mut unused = items.clone();
        assert!(!consume_ignore(
            source,
            Span::new(start, start + 5),
            &[comment],
            &items,
            &mut unused,
            |_| true
        ));
        assert_eq!(unused.len(), 1);
    }

    #[test]
    fn only_the_nearest_matching_comment_code_is_consumed() {
        let source = "<!-- svelte-ignore foo -->\n<!-- svelte-ignore foo -->\ntext > text<img>";
        let comments = [Span::new(0, 26), Span::new(27, 53)];
        let mut items = Vec::new();
        for comment in comments {
            parse_comment(
                " svelte-ignore foo ",
                comment.start + 4,
                comment,
                &mut items,
            );
        }
        items.sort_by_key(|item| std::cmp::Reverse(item.span.start));
        let mut unused = items.clone();
        let start = source.find("<img>").unwrap() as u32;
        assert!(consume_ignore(
            source,
            Span::new(start, start + 5),
            &comments,
            &items,
            &mut unused,
            |item| item.code.as_deref() == Some("foo")
        ));
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].token_span, comments[0]);
    }
}
