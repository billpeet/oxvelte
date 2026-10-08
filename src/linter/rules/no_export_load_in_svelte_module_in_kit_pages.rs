//! `svelte/no-export-load-in-svelte-module-in-kit-pages` — disallow exporting load in SvelteKit page module scripts.
//! ⭐ Recommended (SvelteKit)

use crate::linter::{LintContext, Rule};
use oxc::ast::ast::{BindingPattern, Declaration, Statement};
use oxc::span::Span;

pub struct NoExportLoadInSvelteModuleInKitPages;

impl Rule for NoExportLoadInSvelteModuleInKitPages {
    fn name(&self) -> &'static str {
        "svelte/no-export-load-in-svelte-module-in-kit-pages"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if let Some(fp) = &ctx.file_path {
            let fname = fp.rsplit(['/', '\\']).next().unwrap_or(fp);
            let is_page = ["+page.svelte", "+layout.svelte", "+error.svelte"]
                .iter()
                .any(|s| fname == *s);
            if !is_page {
                return;
            }
            if let Some(routes_dir) = ctx
                .config
                .settings
                .as_ref()
                .and_then(|s| s.get("svelte"))
                .and_then(|s| s.get("kit"))
                .and_then(|s| s.get("files"))
                .and_then(|s| s.get("routes"))
                .and_then(|s| s.as_str())
            {
                if !fp.contains(routes_dir) {
                    return;
                }
            }
        }
        let Some(module_sem) = ctx.module_semantic else {
            return;
        };
        let module_offset = ctx
            .ast
            .module
            .as_ref()
            .map_or(ctx.module_content_offset, |script| {
                script.content_span.start
            });
        for stmt in &module_sem.nodes().program().body {
            let Statement::ExportNamedDeclaration(exp) = stmt else {
                continue;
            };
            let Some(decl) = &exp.declaration else {
                continue;
            };
            let load_span = match decl {
                Declaration::FunctionDeclaration(f) => {
                    if f.id.as_ref().map_or(false, |id| id.name == "load") {
                        f.id.as_ref().map(|id| id.span)
                    } else {
                        None
                    }
                }
                Declaration::VariableDeclaration(vd) => vd.declarations.iter().find_map(|d| {
                    if let BindingPattern::BindingIdentifier(id) = &d.id {
                        if id.name == "load" {
                            return Some(id.span);
                        }
                    }
                    None
                }),
                _ => None,
            };
            if let Some(span) = load_span {
                let s = module_offset + span.start;
                let e = module_offset + span.end;
                ctx.diagnostic(
                    "disallow exporting load functions in `*.svelte` module in SvelteKit page components.",
                    Span::new(s, e),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_locations_survive_unicode_attributes_and_windows_paths() {
        let source = "<!-- 😀 --><script context=\"module\" data-note=\">\">export function load() {} export function helper() { const load = 1; } const local = 1; export { local as loadAlias };</script>";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty());
        for (path, count) in [
            ("C:\\app\\src\\routes\\+page.svelte", 1),
            ("src/routes/not+page.svelte", 0),
        ] {
            let diagnostics: Vec<_> = crate::linter::Linter::all()
                .lint_with_config_and_path(
                    &parsed.ast,
                    source,
                    crate::linter::RuleConfig::default(),
                    path,
                )
                .into_iter()
                .filter(|d| d.rule_name == "svelte/no-export-load-in-svelte-module-in-kit-pages")
                .collect();
            assert_eq!(diagnostics.len(), count);
            if let Some(diagnostic) = diagnostics.first() {
                assert_eq!(
                    &source[diagnostic.span.start as usize..diagnostic.span.end as usize],
                    "load"
                );
            }
        }
    }
}
