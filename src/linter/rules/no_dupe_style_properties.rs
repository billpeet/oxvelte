//! Disallow duplicate inline style properties, preserving alternative branches.
use super::style_declarations::declarations;
use crate::ast::TemplateNode;
use crate::linter::{walk_template_nodes, LintContext, Rule};
use rustc_hash::{FxHashMap, FxHashSet};

pub struct NoDupeStyleProperties;
impl Rule for NoDupeStyleProperties {
    fn name(&self) -> &'static str {
        "svelte/no-dupe-style-properties"
    }
    fn is_recommended(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::Element(el) = node else {
                return;
            };
            let mut before = FxHashMap::default();
            let mut reported = FxHashSet::default();
            for attr in &el.attributes {
                for set in declarations(attr, ctx.source) {
                    for (prop, span) in &set {
                        if let Some(first) = before.get(prop) {
                            for report in [*first, *span] {
                                if reported.insert(report) {
                                    ctx.diagnostic(
                                        format!("Duplicate property '{}'.", prop),
                                        report,
                                    );
                                }
                            }
                        }
                    }
                    for (prop, span) in set {
                        before.insert(prop, span);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternative_properties_are_not_duplicates_of_each_other() {
        for (source, count) in [
            ("<div style={ok ? 'color:red' : 'color:blue'}/>", 0),
            (
                "<div title='é' style=\"color:red; {ok ? 'color:blue' : `color:${x}`}\"/>",
                3,
            ),
        ] {
            let allocator = oxc::allocator::Allocator::default();
            let parsed = crate::parser::parse_for_lint(source, &allocator);
            let mut ctx = LintContext::new(&parsed.ast, source);
            NoDupeStyleProperties.run(&mut ctx);
            let diagnostics = ctx.into_diagnostics();
            assert_eq!(diagnostics.len(), count);
            for diagnostic in diagnostics {
                let span = diagnostic.span;
                assert_eq!(&source[span.start as usize..span.end as usize], "color");
            }
        }
    }
}
