//! Disallow unscoped style elements inside the component template.
use crate::ast::TemplateNode;
use crate::linter::{walk_template_nodes, LintContext, Rule};

pub struct NoNestedStyleTag;
impl Rule for NoNestedStyleTag {
    fn name(&self) -> &'static str {
        "svelte/no-nested-style-tag"
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            if let TemplateNode::Element(element) = node {
                if element.kind().is_html() && element.name == "style" {
                    ctx.diagnostic("Nested `<style>` elements are not scoped and may lead to unintended styles being applied.", element.span);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn root_styles_are_scoped_but_styles_inside_blocks_and_snippets_are_not() {
        let source = "<style>p {color:red}</style><!-- 😀 -->{#if ok}<div><style>p {color:blue}</style></div>{/if}{#snippet row()}<style>p {color:green}</style>{/snippet}";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        let mut ctx = LintContext::new(&parsed.ast, source);
        NoNestedStyleTag.run(&mut ctx);
        let diagnostics = ctx.into_diagnostics();
        let spans: Vec<_> = diagnostics
            .iter()
            .map(|d| &source[d.span.start as usize..d.span.end as usize])
            .collect();
        assert_eq!(
            spans,
            [
                "<style>p {color:blue}</style>",
                "<style>p {color:green}</style>"
            ]
        );
    }
}
