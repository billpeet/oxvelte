//! `svelte/no-dupe-use-directives` — disallow duplicate use directives.
//! ⭐ Recommended

use crate::ast::{Attribute, DirectiveKind, TemplateNode};
use crate::linter::rules::directive_expression_key;
use crate::linter::{walk_template_nodes, LintContext, Rule};

pub struct NoDupeUseDirectives;

impl Rule for NoDupeUseDirectives {
    fn name(&self) -> &'static str {
        "svelte/no-dupe-use-directives"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        use std::collections::HashMap;
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            let mut groups: HashMap<String, Vec<(String, oxc::span::Span)>> = HashMap::new();
            for attr in &el.attributes {
                if let Attribute::Directive {
                    kind: DirectiveKind::Use,
                    name,
                    value,
                    span,
                    ..
                } = attr
                {
                    let expr = directive_expression_key(value);
                    groups.entry(name.clone()).or_default().push((expr, *span));
                }
            }
            let mut diagnostics = Vec::new();
            for (name, entries) in &groups {
                let mut by_expr: HashMap<&str, Vec<oxc::span::Span>> = HashMap::new();
                for (expr, span) in entries {
                    by_expr.entry(expr.as_str()).or_default().push(*span);
                }
                for spans in by_expr.values().filter(|s| s.len() >= 2) {
                    for (index, span) in spans.iter().enumerate() {
                        let other = spans[if index == 0 { 1 } else { 0 }];
                        let line = line_number(ctx.source, other.start as usize);
                        diagnostics.push((
                            *span,
                            format!("This `use:{name}` directive is the same and duplicate directives in L{line}."),
                        ));
                    }
                }
            }
            diagnostics.sort_by_key(|(span, _)| span.start);
            for (span, message) in diagnostics {
                ctx.diagnostic(message, span);
            }
        });
    }
}

fn line_number(source: &str, offset: usize) -> usize {
    let mut line = 1;
    let mut previous_cr = false;
    for character in source[..offset].chars() {
        if matches!(character, '\r' | '\u{2028}' | '\u{2029}') || character == '\n' && !previous_cr
        {
            line += 1;
        }
        previous_cr = character == '\r';
    }
    line
}

#[cfg(test)]
mod tests {
    use super::line_number;
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn partner_line_counts_all_ecmascript_line_terminators() {
        for separator in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
            let source = format!("first{separator}second{separator}third");
            assert_eq!(line_number(&source, source.find("third").unwrap()), 3);
        }
    }

    #[test]
    fn duplicate_actions_report_other_line_and_keep_source_order() {
        let source = "<div\n use:actions.foo={param}\n use:bar\n use:actions.foo={param}\n use:bar\n use:actions.foo={param} />\n<div use:bar />";
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let diagnostics = Linter::all().lint(&parsed.ast, source);
        let diagnostics: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule_name == "svelte/no-dupe-use-directives")
            .collect();
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>(),
            [
                "This `use:actions.foo` directive is the same and duplicate directives in L4.",
                "This `use:bar` directive is the same and duplicate directives in L5.",
                "This `use:actions.foo` directive is the same and duplicate directives in L2.",
                "This `use:bar` directive is the same and duplicate directives in L3.",
                "This `use:actions.foo` directive is the same and duplicate directives in L2.",
            ]
        );
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| &source
                    [diagnostic.span.start as usize..diagnostic.span.end as usize])
                .collect::<Vec<_>>(),
            [
                "use:actions.foo={param}",
                "use:bar",
                "use:actions.foo={param}",
                "use:bar",
                "use:actions.foo={param}",
            ]
        );
    }
}
