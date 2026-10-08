//! `svelte/experimental-require-slot-types` — require slot types to be defined
//! for components that expose slots.

use crate::ast::TemplateNode;
use crate::linter::{walk_template_nodes, LintContext, Rule};
use oxc::ast::ast::Statement;
use oxc::ast::AstKind;
use oxc::span::Span;

pub struct ExperimentalRequireSlotTypes;

fn is_ts(lang: Option<&str>) -> bool {
    lang.map_or(false, |l| {
        l.eq_ignore_ascii_case("ts") || l.eq_ignore_ascii_case("typescript")
    })
}

impl Rule for ExperimentalRequireSlotTypes {
    fn name(&self) -> &'static str {
        "svelte/experimental-require-slot-types"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        // Vendor's `meta.conditions` excludes Svelte 5 runes mode.
        if ctx.is_runes {
            return;
        }
        // Upstream updates its language flag on each script in source order.
        let is_ts_file = [&ctx.ast.instance, &ctx.ast.module]
            .into_iter()
            .flatten()
            .max_by_key(|script| script.span.start)
            .is_some_and(|script| is_ts(script.lang.as_deref()));
        if !is_ts_file {
            return;
        }

        let mut slot_span: Option<Span> = None;
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            if let TemplateNode::Element(el) = node {
                if el.name == "slot" && slot_span.is_none() {
                    slot_span = Some(el.span);
                }
            }
        });
        let Some(_) = slot_span else { return };

        // Check for `$$Slots` declaration in either semantic (instance or module).
        let has_slots = [ctx.instance_semantic, ctx.module_semantic]
            .iter()
            .filter_map(|s| *s)
            .any(|sem| {
                sem.nodes().iter().any(|n| match n.kind() {
                    AstKind::TSInterfaceDeclaration(i) => i.id.name == "$$Slots",
                    AstKind::TSTypeAliasDeclaration(t) => t.id.name == "$$Slots",
                    _ => false,
                }) || sem.nodes().program().body.iter().any(|stmt| match stmt {
                    Statement::TSInterfaceDeclaration(i) => i.id.name == "$$Slots",
                    Statement::TSTypeAliasDeclaration(t) => t.id.name == "$$Slots",
                    _ => false,
                })
            });

        if !has_slots {
            let position = ctx
                .source
                .chars()
                .next()
                .map_or(0, |ch| ch.len_utf8() as u32);
            let span = Span::new(position, position);
            ctx.diagnostic("The component must define the $$Slots interface.", span);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn program_location_and_last_script_language_match_upstream() {
        fn lint(source: &str) -> Vec<crate::linter::LintDiagnostic> {
            let allocator = oxc::allocator::Allocator::default();
            let parsed = crate::parser::parse_for_lint(source, &allocator);
            crate::linter::Linter::all()
                .lint(&parsed.ast, source)
                .into_iter()
                .filter(|d| d.rule_name == "svelte/experimental-require-slot-types")
                .collect()
        }
        let diagnostics = lint("é<script lang=\"ts\"></script><slot/>");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start, 2);
        assert!(lint("<script lang=\"ts\"></script><script module></script><slot/>").is_empty());
        assert_eq!(
            lint("<script module></script><script lang=\"ts\"></script><slot/>").len(),
            1
        );
    }
}
