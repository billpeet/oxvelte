//! `svelte/valid-each-key` — enforce that each blocks with key use a unique identifier.
//! ⭐ Recommended

use crate::ast::TemplateNode;
use crate::linter::{walk_template_nodes, LintContext, Rule};
use crate::parser::expression::parse_template_expression;
use oxc::allocator::Allocator;
use oxc::ast::AstKind;
use oxc::semantic::SemanticBuilder;

pub struct ValidEachKey;

impl Rule for ValidEachKey {
    fn name(&self) -> &'static str {
        "svelte/valid-each-key"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let TemplateNode::EachBlock(block) = node else {
                return;
            };
            let Some(key) = &block.key else { return };
            let key = key.trim();
            let mut iter_vars = extract_iter_vars(&block.context);
            if let Some(index) = &block.index {
                iter_vars.push(index.trim().to_string());
            }
            let uses_var = key_references_each_var(key, &iter_vars);
            if !uses_var {
                ctx.diagnostic(
                    "Expected key to use the variables which are defined by the `{#each}` block.",
                    block.key_span.unwrap_or(block.header_span),
                );
            }
        });
    }
}

fn extract_iter_vars(context: &str) -> Vec<String> {
    let alloc = Allocator::default();
    let source = format!("let {context} = value;");
    let parsed = oxc::parser::Parser::new(&alloc, &source, oxc::span::SourceType::ts()).parse();
    if !parsed.errors.is_empty() {
        return Vec::new();
    }
    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    semantic
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
        })
        .collect()
}

fn key_references_each_var(key: &str, vars: &[String]) -> bool {
    if vars.iter().all(|var| var.is_empty()) {
        return false;
    }

    let alloc = Allocator::default();
    let parsed = parse_template_expression(key, &alloc);
    if !parsed.errors.is_empty() {
        return false;
    }
    let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
    let references_each_var = semantic.nodes().iter().any(|node| {
        let AstKind::IdentifierReference(id) = node.kind() else {
            return false;
        };
        semantic
            .scoping()
            .get_reference(id.reference_id())
            .symbol_id()
            .is_none()
            && vars.iter().any(|var| id.name == var.as_str())
    });
    references_each_var
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
            .filter(|d| d.rule_name == "svelte/valid-each-key")
            .collect()
    }

    #[test]
    fn parity_regression_preserves_source_boundaries() {
        let source = "<!-- 😀 -->{#each values as {nested: {id}} ((() => {const id = 1; return id;})())}x{/each}";
        let diagnostics = lint(source, serde_json::json!([]));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "(() => {const id = 1; return id;})()"
        );
        assert!(lint(
            "{#each values as {nested: {id}} (id)}x{/each}",
            serde_json::json!([])
        )
        .is_empty());
    }
}
