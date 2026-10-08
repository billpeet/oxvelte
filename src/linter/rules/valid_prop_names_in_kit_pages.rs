//! `svelte/valid-prop-names-in-kit-pages` — ensure exported props in SvelteKit pages
//! use valid names (`data`, `form`, `snapshot`).
//! ⭐ Recommended

use crate::linter::{LintContext, Rule};
use oxc::ast::ast::{BindingPattern, Declaration, ExportNamedDeclaration, Expression, Statement};
use oxc::span::{GetSpan, Span};

const PAGE_PROPS: &[&str] = &["data", "form", "params", "snapshot"];
const LEGACY_PROPS: &[&str] = &["data", "errors", "form", "params", "snapshot"];
const LAYOUT_PROPS: &[&str] = &["data", "form", "params", "snapshot", "children"];
const ERROR_PROPS: &[&str] = &["error"];

pub struct ValidPropNamesInKitPages;

impl Rule for ValidPropNamesInKitPages {
    fn name(&self) -> &'static str {
        "svelte/valid-prop-names-in-kit-pages"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let Some(file_path) = &ctx.file_path else {
            return;
        };
        let fname = file_path.rsplit('/').next().unwrap_or(file_path);
        let fname = fname.rsplit('\\').next().unwrap_or(fname);
        if fname != "+page.svelte" && fname != "+layout.svelte" && fname != "+error.svelte" {
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
            if !file_path.contains(routes_dir) {
                return;
            }
        }

        let Some(semantic) = ctx.instance_semantic else {
            return;
        };
        let content_offset = ctx.ast.instance.as_ref().unwrap().content_span.start;
        let svelte5 = ctx.svelte_version.is_unknown() || ctx.svelte_version.includes_major(5);
        let valid_props = match (svelte5, fname) {
            (true, "+layout.svelte") => LAYOUT_PROPS,
            (true, "+error.svelte") => ERROR_PROPS,
            _ => PAGE_PROPS,
        };

        for stmt in &semantic.nodes().program().body {
            match stmt {
                // `export let name;` / `export let name = init;` — Svelte 3/4 props.
                Statement::ExportNamedDeclaration(exp) => {
                    check_export_named(ctx, content_offset, exp);
                }
                // `let { ... } = $props();` — Svelte 5 props.
                Statement::VariableDeclaration(vd) => {
                    for d in &vd.declarations {
                        let is_props_call = d.init.as_ref().map_or(false, |init| match init {
                            Expression::CallExpression(ce) => matches!(
                                &ce.callee,
                                Expression::Identifier(id) if id.name == "$props"
                            ),
                            _ => false,
                        });
                        if is_props_call {
                            report_pattern_names(ctx, content_offset, &d.id, valid_props);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn check_export_named<'a>(
    ctx: &mut LintContext<'_>,
    content_offset: u32,
    exp: &'a ExportNamedDeclaration<'a>,
) {
    let Some(decl) = &exp.declaration else { return };
    let Declaration::VariableDeclaration(vd) = decl else {
        return;
    };
    for d in &vd.declarations {
        if let BindingPattern::BindingIdentifier(id) = &d.id {
            if !LEGACY_PROPS.contains(&id.name.as_str()) {
                report(ctx, content_offset, d.span);
            }
        } else {
            report_pattern_names(ctx, content_offset, &d.id, LEGACY_PROPS);
        }
    }
}

fn report_pattern_names<'a>(
    ctx: &mut LintContext<'_>,
    content_offset: u32,
    pat: &BindingPattern<'a>,
    valid: &[&str],
) {
    match pat {
        BindingPattern::BindingIdentifier(_) => {}
        BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                let key_name = match &prop.key {
                    oxc::ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => continue,
                };
                if !valid.contains(&key_name) {
                    report(ctx, content_offset, prop.key.span());
                }
            }
        }
        BindingPattern::ArrayPattern(_) => {
            // Array destructuring of $props is invalid anyway; skip.
        }
        BindingPattern::AssignmentPattern(inner) => {
            // `{ custom = 'default' }` — walk into the left-hand pattern.
            report_pattern_names(ctx, content_offset, &inner.left, valid);
        }
    }
}

fn report(ctx: &mut LintContext<'_>, content_offset: u32, span: Span) {
    let s = content_offset + span.start;
    let e = content_offset + span.end;
    ctx.diagnostic(
        "disallow invalid props in SvelteKit route components.",
        Span::new(s, e),
    );
}

#[cfg(test)]
mod tests {
    use crate::linter::{LintDiagnostic, Linter, RuleConfig};
    use crate::parser;
    use oxc::allocator::Allocator;

    fn lint(source: &str, path: &str) -> Vec<LintDiagnostic> {
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint_with_config_and_path(&parsed.ast, source, RuleConfig::default(), path)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/valid-prop-names-in-kit-pages")
            .collect()
    }

    #[test]
    fn rune_props_depend_on_the_route_kind() {
        let source = r#"<!-- 😀 -->
        <script module>const { moduleProp } = $props();</script>
        <script data-note=">">
            const { data, form, params, snapshot, children, error, errors, ...rest } = $props();
        </script>"#;
        for (path, expected) in [
            (
                "src/routes/+page.svelte",
                vec!["children", "error", "errors"],
            ),
            ("src/routes/+layout.svelte", vec!["error", "errors"]),
            (
                "src/routes/+error.svelte",
                vec!["data", "form", "params", "snapshot", "children", "errors"],
            ),
            ("src/lib/Component.svelte", vec![]),
        ] {
            let diagnostics = lint(source, path);
            assert_eq!(
                diagnostics
                    .iter()
                    .map(|d| &source[d.span.start as usize..d.span.end as usize])
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(diagnostics
                .iter()
                .all(|d| d.message == "disallow invalid props in SvelteKit route components."));
        }
    }

    #[test]
    fn rest_quoted_keys_and_whole_bindings_are_not_named_props() {
        let source = r#"<script>
            let { 'custom': custom, ...rest } = $props();
            const props = $props();
        </script>"#;
        assert!(lint(source, "src/routes/+page.svelte").is_empty());
    }

    #[test]
    fn legacy_exports_keep_legacy_names_and_report_the_declarator() {
        let source = r#"<script>export let errors; export let custom = 'value';</script>"#;
        let diagnostics = lint(source, "src/routes/+page.svelte");
        assert_eq!(diagnostics.len(), 1);
        let span = diagnostics[0].span;
        assert_eq!(
            &source[span.start as usize..span.end as usize],
            "custom = 'value'"
        );
    }
}
