//! `svelte/sort-attributes` — enforce attribute sorting order.
use crate::ast::{Attribute, DirectiveKind, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use oxc::span::Span;
use regex::{Regex, RegexBuilder};
use std::cmp::Ordering;

pub struct SortAttributes;
impl Rule for SortAttributes {
    fn name(&self) -> &'static str {
        "svelte/sort-attributes"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let rules = parse_order_config(&ctx.config.options);
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            let keys: Vec<_> = el.attributes.iter().map(attribute_key).collect();
            let mut valid_previous = Vec::new();
            for (index, key) in keys.iter().enumerate() {
                let Some(key) = key else { continue };
                if !rules.iter().any(|r| r.matches(key)) {
                    continue;
                }
                let invalid = valid_previous.iter().copied().find(|&previous: &usize| {
                    compare(keys[previous].as_ref().unwrap(), key, &rules) == Ordering::Greater
                });
                if let Some(mut previous) = invalid {
                    let normal = matches!(&el.attributes[index], Attribute::NormalAttribute { name, .. } if name != "@attach");
                    if normal
                        && el.attributes[previous..index]
                            .iter()
                            .any(|a| matches!(a, Attribute::Spread { .. }))
                    {
                        let start = (0..index)
                            .rev()
                            .find(|&i| matches!(el.attributes[i], Attribute::Spread { .. }))
                            .map_or(0, |i| i + 1);
                        let Some(local) = (start..index).find(|&i| {
                            keys[i].as_ref().is_some_and(|k| {
                                rules.iter().any(|r| r.matches(k))
                                    && compare(k, key, &rules) == Ordering::Greater
                            })
                        }) else {
                            continue;
                        };
                        previous = local;
                    }
                    let span = attribute_span(&el.attributes[index], ctx.source);
                    let first = attribute_span(&el.attributes[previous], ctx.source);
                    let mut replacement =
                        ctx.source[span.start as usize..span.end as usize].to_string();
                    // Rotate whole attributes, retaining whitespace in each original slot.
                    for i in previous..index {
                        let current = attribute_span(&el.attributes[i], ctx.source);
                        let next = attribute_span(&el.attributes[i + 1], ctx.source);
                        replacement
                            .push_str(&ctx.source[current.end as usize..next.start as usize]);
                        replacement
                            .push_str(&ctx.source[current.start as usize..current.end as usize]);
                    }
                    ctx.diagnostic_with_fix(
                        format!(
                            "Attribute '{}' should go before '{}'.",
                            key,
                            keys[previous].as_ref().unwrap()
                        ),
                        span,
                        Fix {
                            span: Span::new(first.start, span.end),
                            replacement,
                        },
                    );
                } else {
                    valid_previous.push(index);
                }
            }
        });
    }
}
fn attribute_span(attribute: &Attribute, source: &str) -> Span {
    let span = match attribute {
        Attribute::NormalAttribute { span, .. }
        | Attribute::Directive { span, .. }
        | Attribute::Spread { span } => *span,
    };
    // Mustache attribute spans can include whitespace consumed after the closing brace.
    let text = &source[span.start as usize..span.end as usize];
    Span::new(span.start, span.start + text.trim_end().len() as u32)
}
fn attribute_key(attribute: &Attribute) -> Option<String> {
    match attribute {
        Attribute::NormalAttribute { name, .. } => Some(name.clone()),
        Attribute::Spread { .. } => None,
        Attribute::Directive { kind, name, .. } => {
            Some(format!("{}:{}", directive_prefix(kind), name))
        }
    }
}
fn directive_prefix(kind: &DirectiveKind) -> &'static str {
    match kind {
        DirectiveKind::EventHandler => "on",
        DirectiveKind::Binding => "bind",
        DirectiveKind::Class => "class",
        DirectiveKind::StyleDirective => "style",
        DirectiveKind::Use => "use",
        DirectiveKind::Transition => "transition",
        DirectiveKind::In => "in",
        DirectiveKind::Out => "out",
        DirectiveKind::Animate => "animate",
        DirectiveKind::Let => "let",
    }
}
struct Matcher {
    negative: bool,
    regex: Option<Regex>,
}
struct OrderRule {
    patterns: Vec<Matcher>,
    alphabetical: bool,
}
impl OrderRule {
    fn matches(&self, name: &str) -> bool {
        let mut result = self.patterns.first().is_some_and(|p| p.negative);
        for pattern in &self.patterns {
            if result == pattern.negative
                && pattern.regex.as_ref().is_some_and(|r| r.is_match(name))
            {
                result = !pattern.negative;
            }
        }
        result
    }
}
fn compare(a: &str, b: &str, rules: &[OrderRule]) -> Ordering {
    for rule in rules {
        match (rule.matches(a), rule.matches(b)) {
            (true, true) => {
                return if rule.alphabetical {
                    a.encode_utf16().cmp(b.encode_utf16())
                } else {
                    Ordering::Equal
                }
            }
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
    }
    Ordering::Equal
}
fn compile_matcher(pattern: &str) -> Matcher {
    let (negative, pattern) = pattern
        .strip_prefix('!')
        .map_or((false, pattern), |p| (true, p));
    let regex = if let Some(rest) = pattern.strip_prefix('/') {
        if let Some((expression, flags)) = rest.rsplit_once('/') {
            RegexBuilder::new(expression)
                .case_insensitive(flags.contains('i'))
                .multi_line(flags.contains('m'))
                .dot_matches_new_line(flags.contains('s'))
                .build()
                .ok()
        } else {
            Regex::new(&format!("^{}$", regex::escape(pattern))).ok()
        }
    } else {
        Regex::new(&format!("^{}$", regex::escape(pattern))).ok()
    };
    Matcher { negative, regex }
}
fn parse_order_config(options: &Option<serde_json::Value>) -> Vec<OrderRule> {
    let default = serde_json::json!([
        "this", "bind:this", "id", "name", "slot",
        {"match":"/^--/u","sort":"alphabetical"}, ["style","/^style:/u"], "class",
        {"match":"/^class:/u","sort":"alphabetical"},
        {"match":["!/:/u","!/^(?:this|id|name|style|class)$/u","!/^--/u"],"sort":"alphabetical"},
        ["/^bind:/u","!bind:this","/^on:/u"],
        {"match":"/^use:/u","sort":"alphabetical"}, {"match":"/^transition:/u","sort":"alphabetical"},
        {"match":"/^in:/u","sort":"alphabetical"}, {"match":"/^out:/u","sort":"alphabetical"},
        {"match":"/^animate:/u","sort":"alphabetical"}, {"match":"/^let:/u","sort":"alphabetical"}
    ]);
    let order = options
        .as_ref()
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("order"))
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| default.as_array().unwrap());
    order
        .iter()
        .map(|entry| {
            let patterns = entry.get("match").unwrap_or(entry);
            let patterns: Vec<_> = if let Some(pattern) = patterns.as_str() {
                vec![compile_matcher(pattern)]
            } else {
                patterns
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                    .map(compile_matcher)
                    .collect()
            };
            OrderRule {
                patterns,
                alphabetical: entry.get("sort").and_then(|v| v.as_str()) == Some("alphabetical"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        linter::{LintDiagnostic, Linter, RuleConfig},
        parser,
    };
    use oxc::allocator::Allocator;

    fn lint(source: &str, options: serde_json::Value) -> Vec<LintDiagnostic> {
        let allocator = Allocator::default();
        let parsed = parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint_with_config(
                &parsed.ast,
                source,
                RuleConfig {
                    options: Some(options),
                    ..RuleConfig::default()
                },
            )
            .into_iter()
            .filter(|d| d.rule_name == "svelte/sort-attributes")
            .collect()
    }

    #[test]
    fn insertion_diagnostics_keep_duplicate_attributes_and_case_sensitive_order() {
        let source = "<div z a a A></div>";
        let diagnostics = lint(source, serde_json::json!([]));
        assert_eq!(diagnostics.len(), 3);
        for diagnostic in diagnostics {
            assert!(diagnostic.message.ends_with("before 'z'."));
            assert_eq!(
                &source[diagnostic.span.start as usize..diagnostic.span.end as usize],
                if diagnostic.message.starts_with("Attribute 'A'") {
                    "A"
                } else {
                    "a"
                }
            );
        }
    }

    #[test]
    fn normal_attributes_stop_at_spreads_but_directives_can_cross_them() {
        let normal = lint("<div z {...props} a></div>", serde_json::json!([]));
        assert!(normal.is_empty());
        let source = "<div on:click {...props} class:active></div>";
        let diagnostics = lint(source, serde_json::json!([]));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].fix.as_ref().unwrap().replacement,
            "class:active on:click {...props}"
        );
    }

    #[test]
    fn rotation_preserves_slot_whitespace_and_unicode_ranges() {
        let source = "<!-- 😀 --><div z='é'\n\tignored=\"x > y\"  a={value}></div>";
        let diagnostics = lint(
            source,
            serde_json::json!([{"order":[{"match":["z","a"],"sort":"alphabetical"}]}]),
        );
        assert_eq!(diagnostics.len(), 1);
        let fix = diagnostics[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "a={value}\n\tz='é'  ignored=\"x > y\"");
        assert_eq!(
            &source[fix.span.start as usize..fix.span.end as usize],
            "z='é'\n\tignored=\"x > y\"  a={value}"
        );
    }

    #[test]
    fn ordered_negative_patterns_can_reinclude_names_and_empty_order_ignores_all() {
        let rules = parse_order_config(&Some(
            serde_json::json!([{"order":[{"match":["!/^x/i","x-allowed"],"sort":"alphabetical"}]}]),
        ));
        assert!(!rules[0].matches("X-denied"));
        assert!(rules[0].matches("x-allowed"));
        assert!(rules[0].matches("other"));
        assert!(lint("<div z a></div>", serde_json::json!([{"order":[]}])).is_empty());
    }

    #[test]
    fn attachment_has_its_own_key_and_moves_as_one_attribute() {
        let source = "<div foo {@attach attach}></div>";
        let diagnostics = lint(source, serde_json::json!([]));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Attribute '@attach' should go before 'foo'."
        );
        assert_eq!(
            diagnostics[0].fix.as_ref().unwrap().replacement,
            "{@attach attach} foo"
        );
    }
}
