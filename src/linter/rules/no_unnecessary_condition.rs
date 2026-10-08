//! Opt-in TypeScript unnecessary-condition checks with Svelte reactive exclusions.
use crate::ast::TemplateNode;
use crate::compiler::{byte_utf16, utf16_byte};
use crate::linter::{Fix, LintContext, Rule};
use oxc::span::Span;
use serde::Deserialize;
use serde_json::{json, Value};

pub struct NoUnnecessaryCondition;

#[derive(Deserialize)]
struct Diagnostic {
    message: String,
    start: usize,
    end: usize,
    fix: Option<Replacement>,
}
#[derive(Deserialize)]
struct Replacement {
    start: usize,
    end: usize,
    text: String,
}

impl Rule for NoUnnecessaryCondition {
    fn name(&self) -> &'static str {
        "@typescript-eslint/no-unnecessary-condition"
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if ctx.is_svelte_module
            || !ctx
                .file_path
                .as_deref()
                .is_some_and(|path| path.ends_with(".svelte"))
            || ctx
                .ast
                .instance
                .iter()
                .chain(ctx.ast.module.iter())
                .all(|script| !matches!(script.lang.as_deref(), Some("ts" | "typescript")))
        {
            return;
        }
        let scripts: Vec<_> = ctx
            .ast
            .instance
            .iter()
            .chain(ctx.ast.module.iter())
            .map(|script| {
                json!({
                    "start": byte_utf16(ctx.source, script.content_span.start),
                    "end": byte_utf16(ctx.source, script.content_span.end),
                    "lang": script.lang,
                    "module": script.module,
                })
            })
            .collect();
        let mut templates = Vec::new();
        collect_templates(ctx.source, &ctx.ast.html.nodes, &mut templates);
        let request = json!({
            "operation": "typescript-conditions", "source": ctx.source,
            "filename": ctx.file_path, "settings": ctx.config.settings,
            "scripts": scripts, "templates": templates,
            "options": ctx.config.options.as_ref().and_then(Value::as_array).and_then(|options| options.first()),
        });
        let result = crate::compiler::typescript_conditions(&request).and_then(|value| {
            serde_json::from_value::<Vec<Diagnostic>>(value).map_err(|error| error.to_string())
        });
        match result {
            Err(error) => ctx.diagnostic(
                format!("Unable to run TypeScript checks: {error}"),
                Span::new(0, 0),
            ),
            Ok(diagnostics) => {
                let length = ctx.source.encode_utf16().count();
                for diagnostic in diagnostics {
                    if diagnostic.start > diagnostic.end
                        || diagnostic.end > length
                        || diagnostic
                            .fix
                            .as_ref()
                            .is_some_and(|fix| fix.start > fix.end || fix.end > length)
                    {
                        ctx.diagnostic(
                            "TypeScript checker returned an invalid source range",
                            Span::new(0, 0),
                        );
                        continue;
                    }
                    let span = Span::new(
                        utf16_byte(ctx.source, diagnostic.start),
                        utf16_byte(ctx.source, diagnostic.end),
                    );
                    if let Some(fix) = diagnostic.fix {
                        ctx.diagnostic_with_fix(
                            diagnostic.message,
                            span,
                            Fix {
                                span: Span::new(
                                    utf16_byte(ctx.source, fix.start),
                                    utf16_byte(ctx.source, fix.end),
                                ),
                                replacement: fix.text,
                            },
                        );
                    } else {
                        ctx.diagnostic(diagnostic.message, span);
                    }
                }
            }
        }
    }
}

fn add(source: &str, templates: &mut Vec<Value>, span: Span, kind: &str) {
    if span.start < span.end && source.get(span.start as usize..span.end as usize).is_some() {
        templates.push(json!({"start": byte_utf16(source, span.start), "end": byte_utf16(source, span.end), "kind": kind}));
    }
}

/// Project expressions whose bindings come from the script. Template-local
/// binding scopes require a scoped projection and are conservatively omitted.
fn collect_templates(source: &str, nodes: &[TemplateNode<'_>], templates: &mut Vec<Value>) {
    for node in nodes {
        match node {
            TemplateNode::Element(element) => {
                for meta in &element.attribute_meta {
                    if let Some(span) = meta.expression_span {
                        add(source, templates, span, "expression");
                    }
                    for part in &meta.parts {
                        if let Some(span) = part.expression_span {
                            add(source, templates, span, "expression");
                        }
                    }
                }
                collect_templates(source, &element.children, templates);
            }
            TemplateNode::MustacheTag(tag) => {
                add(source, templates, tag.expression_span, "expression")
            }
            TemplateNode::RawMustacheTag(tag) => {
                add(source, templates, tag.expression_span, "expression")
            }
            TemplateNode::RenderTag(tag) => {
                add(source, templates, tag.expression_span, "expression")
            }
            TemplateNode::IfBlock(block) => {
                add(source, templates, block.test_span, "condition");
                collect_templates(source, &block.consequent.nodes, templates);
                if let Some(alternate) = &block.alternate {
                    collect_templates(source, std::slice::from_ref(alternate.as_ref()), templates);
                }
            }
            TemplateNode::EachBlock(block) => {
                add(source, templates, block.expression_span, "expression");
                if let Some(fallback) = &block.fallback {
                    collect_templates(source, &fallback.nodes, templates);
                }
            }
            TemplateNode::AwaitBlock(block) => {
                add(source, templates, block.expression_span, "expression");
                if let Some(pending) = &block.pending {
                    collect_templates(source, &pending.nodes, templates);
                }
                if block.then_binding.is_none() {
                    if let Some(branch) = &block.then {
                        collect_templates(source, &branch.nodes, templates);
                    }
                }
                if block.catch_binding.is_none() {
                    if let Some(branch) = &block.catch {
                        collect_templates(source, &branch.nodes, templates);
                    }
                }
            }
            TemplateNode::KeyBlock(block) => {
                add(source, templates, block.expression_span, "expression");
                collect_templates(source, &block.body.nodes, templates);
            }
            TemplateNode::ConstTag(_) => break,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn template_projection_uses_native_ranges_and_omits_local_binding_scopes() {
        let source = "<script lang='ts'>const value = false;</script>{#if value}<p title={value || 1}>{value ?? 2}</p>{/if}{#each [] as value}{value?.x}{/each}{#snippet item(value)}{value?.x}{/snippet}";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        let mut templates = Vec::new();
        collect_templates(source, &parsed.ast.html.nodes, &mut templates);
        let expressions: Vec<_> = templates
            .iter()
            .map(|query| {
                let start = utf16_byte(source, query["start"].as_u64().unwrap() as usize);
                let end = utf16_byte(source, query["end"].as_u64().unwrap() as usize);
                &source[start as usize..end as usize]
            })
            .collect();
        assert_eq!(expressions, ["value", "value || 1", "value ?? 2", "[]"]);
        assert_eq!(templates[0]["kind"], "condition");
    }
}
