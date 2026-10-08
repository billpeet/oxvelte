//! `svelte/max-lines-per-block` — enforce a maximum number of lines in script/style blocks.

use crate::linter::{LintContext, Rule};
use oxc::span::Span;

pub struct MaxLinesPerBlock;

fn count_lines(content: &str, skip_blank: bool, skip_comments: bool) -> usize {
    let inner: Vec<_> = content.split('\n').collect();
    let inner = if inner.len() >= 2 {
        &inner[1..inner.len() - 1]
    } else {
        &inner[0..0]
    };
    if inner.is_empty() {
        return 0;
    }
    if !skip_blank && !skip_comments {
        return inner.len();
    }
    if !skip_comments {
        return inner.iter().filter(|l| !l.trim().is_empty()).count();
    }
    let mut in_block = false;
    inner
        .iter()
        .filter(|line| {
            let l = line.trim();
            !(skip_blank && l.is_empty()) && !classify_line(l, &mut in_block)
        })
        .count()
}

fn classify_line(line: &str, in_block: &mut bool) -> bool {
    if *in_block {
        return if let Some(end) = line.find("*/") {
            *in_block = false;
            line[end + 2..].trim().is_empty()
        } else {
            true
        };
    }
    if line.starts_with("//") {
        return true;
    }
    if line.starts_with("<!--") {
        return !line.contains("-->") || line[line.find("-->").unwrap() + 3..].trim().is_empty();
    }
    if line.starts_with("/*") {
        return if let Some(end) = line.find("*/") {
            line[end + 2..].trim().is_empty()
        } else {
            *in_block = true;
            true
        };
    }
    false
}

impl Rule for MaxLinesPerBlock {
    fn name(&self) -> &'static str {
        "svelte/max-lines-per-block"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let opts = ctx
            .config
            .options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|a| a.first());
        let get_bool = |k| {
            opts.and_then(|o| o.get(k))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        let get_limit = |k| {
            opts.and_then(|o| o.get(k))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
        };
        let (skip_blank, skip_comments) = (get_bool("skipBlankLines"), get_bool("skipComments"));
        let (script_limit, style_limit, template_limit) = (
            get_limit("script"),
            get_limit("style"),
            get_limit("template"),
        );

        let blocks: Vec<(&str, oxc::span::Span, Option<usize>, &str)> = [
            ctx.ast
                .instance
                .as_ref()
                .map(|s| (s.content.as_str(), s.span, script_limit, "script")),
            ctx.ast
                .module
                .as_ref()
                .map(|s| (s.content.as_str(), s.span, script_limit, "script")),
            ctx.ast
                .css
                .as_ref()
                .map(|s| (s.content.as_str(), s.span, style_limit, "style")),
        ]
        .into_iter()
        .flatten()
        .collect();
        for (content, span, limit, tag) in blocks {
            if let Some(max) = limit {
                let lc = count_lines(content, skip_blank, skip_comments);
                if lc > max {
                    ctx.diagnostic(
                        format!(
                            "<{tag}> block has too many lines ({lc}). Maximum allowed is {max}."
                        ),
                        span,
                    );
                }
            }
        }
        if let Some(max) = template_limit {
            let tc = extract_template_content(ctx.source, ctx);
            let lc = count_template_lines(&tc, skip_blank, skip_comments);
            if lc > max {
                let Some(span) = ctx.ast.html.nodes.iter()
                    .find(|node| !matches!(node, crate::ast::TemplateNode::Element(el) if el.name == "svelte:options"))
                    .map(template_span) else { return };

                ctx.diagnostic(
                    format!("template block has too many lines ({lc}). Maximum allowed is {max}."),
                    span,
                );
            }
        }
    }
}

fn template_span(node: &crate::ast::TemplateNode<'_>) -> Span {
    use crate::ast::TemplateNode;
    match node {
        TemplateNode::Text(node) => node.span,
        TemplateNode::Element(node) => node.span,
        TemplateNode::MustacheTag(node) => node.span,
        TemplateNode::RawMustacheTag(node) => node.span,
        TemplateNode::DebugTag(node) => node.span,
        TemplateNode::ConstTag(node) => node.span,
        TemplateNode::RenderTag(node) => node.span,
        TemplateNode::Comment(node) => node.span,
        TemplateNode::IfBlock(node) => node.span,
        TemplateNode::EachBlock(node) => node.span,
        TemplateNode::AwaitBlock(node) => node.span,
        TemplateNode::KeyBlock(node) => node.span,
        TemplateNode::SnippetBlock(node) => node.span,
    }
}

fn extract_template_content(source: &str, ctx: &LintContext) -> String {
    let line_at = |offset: u32| {
        source[..offset as usize]
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
    };
    let mut excluded = std::collections::HashSet::new();
    let mut exclude = |span: Span| {
        for line in line_at(span.start)..=line_at(span.end) {
            excluded.insert(line);
        }
    };
    for script in [&ctx.ast.instance, &ctx.ast.module].into_iter().flatten() {
        exclude(script.span);
    }
    if let Some(style) = &ctx.ast.css {
        exclude(style.span);
    }
    for node in &ctx.ast.html.nodes {
        if let crate::ast::TemplateNode::Element(el) = node {
            if el.name == "svelte:options" {
                exclude(el.span);
            }
        }
    }
    source
        .split('\n')
        .enumerate()
        .filter(|(line, _)| !excluded.contains(line))
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn count_template_lines(content: &str, skip_blank: bool, skip_comments: bool) -> usize {
    let mut in_comment = false;
    content
        .split('\n')
        .filter(|line| {
            let line = line.trim();
            if skip_blank && line.is_empty() {
                return false;
            }
            if skip_comments {
                if in_comment {
                    if let Some(end) = line.find("-->") {
                        in_comment = false;
                        return !line[end + 3..].trim().is_empty();
                    }
                    return false;
                }
                if line.starts_with("<!--") {
                    if let Some(end) = line.find("-->") {
                        return !line[end + 3..].trim().is_empty();
                    }
                    in_comment = true;
                    return false;
                }
            }
            true
        })
        .count()
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
            .filter(|d| d.rule_name == "svelte/max-lines-per-block")
            .collect()
    }

    #[test]
    fn template_location_uses_the_first_retained_node() {
        let source = "<style>p { color: red; }</style>\n<p>first</p>\n<script>let value = 1;</script>\n<script module>const shared = 1;</script>\n<p>last</p>\n";
        let diagnostics = lint(source, serde_json::json!([{ "template": 1 }]));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].span.start as usize,
            source.find("</style>").unwrap() + 8
        );
    }

    #[test]
    fn parity_regression_preserves_source_boundaries() {
        let source =
            "<script>\n\nlet value = 1;\n\n</script>\n<svelte:options runes={true} />\n<p>😀</p>\n";
        let diagnostics = lint(source, serde_json::json!([{"script":1,"template":1}]));
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("<script> block has too many lines (3)")));
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("template block has too many lines (2)")));
    }
}
