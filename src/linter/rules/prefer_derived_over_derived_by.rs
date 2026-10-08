//! `svelte/prefer-derived-over-derived-by` simplifies callbacks returning one expression.

use crate::ast::TemplateNode;
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use crate::parser::expression::parse_template_expression;
use oxc::allocator::Allocator;
use oxc::ast::ast::{CallExpression, Expression, Statement};
use oxc::ast::AstKind;
use oxc::semantic::SemanticBuilder;
use oxc::span::{GetSpan, Span};

pub struct PreferDerivedOverDerivedBy;

impl Rule for PreferDerivedOverDerivedBy {
    fn name(&self) -> &'static str {
        "svelte/prefer-derived-over-derived-by"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn applies_to_svelte_scripts(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if ctx.runes_explicit_false
            || (!ctx.svelte_version.is_unknown() && !ctx.svelte_version.includes_major(5))
        {
            return;
        }
        for (semantic, script, fallback) in [
            (
                ctx.instance_semantic,
                ctx.ast.instance.as_ref(),
                ctx.instance_content_offset,
            ),
            (
                ctx.module_semantic,
                ctx.ast.module.as_ref(),
                ctx.module_content_offset,
            ),
        ] {
            let Some(semantic) = semantic else { continue };
            let offset = script.map_or(fallback, |s| s.content_span.start) as i64;
            for node in semantic.nodes().iter() {
                if let AstKind::CallExpression(call) = node.kind() {
                    check_call(call, offset, ctx);
                }
            }
        }
        let mut expressions = Vec::new();
        let mut declarations = Vec::new();
        walk_template_nodes(&ctx.ast.html, &mut |node| match node {
            TemplateNode::ConstTag(tag) => declarations.push(tag.declaration_span),
            TemplateNode::MustacheTag(tag) => expressions.push(tag.expression_span),
            TemplateNode::RawMustacheTag(tag) => expressions.push(tag.expression_span),
            TemplateNode::RenderTag(tag) => expressions.push(tag.expression_span),
            TemplateNode::IfBlock(block) => expressions.push(block.test_span),
            TemplateNode::EachBlock(block) => {
                expressions.push(block.expression_span);
                expressions.extend(block.key_span);
            }
            TemplateNode::AwaitBlock(block) => expressions.push(block.expression_span),
            TemplateNode::KeyBlock(block) => expressions.push(block.expression_span),
            TemplateNode::Element(el) => {
                for meta in &el.attribute_meta {
                    expressions.extend(meta.expression_span);
                    expressions.extend(meta.parts.iter().filter_map(|part| part.expression_span));
                }
            }
            _ => {}
        });
        expressions.sort_by_key(|span| (span.start, span.end));
        expressions.dedup();
        for span in expressions {
            let allocator = Allocator::default();
            let parsed = parse_template_expression(
                &ctx.source[span.start as usize..span.end as usize],
                &allocator,
            );
            if !parsed.errors.is_empty() {
                continue;
            }
            let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
            for node in semantic.nodes().iter() {
                if let AstKind::CallExpression(call) = node.kind() {
                    check_call(call, i64::from(span.start) - 6, ctx);
                }
            }
        }
        for span in declarations {
            let allocator = Allocator::default();
            let source = format!(
                "const {};",
                &ctx.source[span.start as usize..span.end as usize]
            );
            let parsed =
                oxc::parser::Parser::new(&allocator, &source, oxc::span::SourceType::ts()).parse();
            if !parsed.errors.is_empty() {
                continue;
            }
            let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
            for node in semantic.nodes().iter() {
                if let AstKind::CallExpression(call) = node.kind() {
                    check_call(call, i64::from(span.start) - 6, ctx);
                }
            }
        }
    }
}

fn check_call(call: &CallExpression<'_>, offset: i64, ctx: &mut LintContext<'_>) {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return;
    };
    if member.property.name != "by"
        || !matches!(&member.object, Expression::Identifier(id) if id.name == "$derived")
        || call.arguments.len() != 1
    {
        return;
    }
    let Some(argument) = call.arguments[0].as_expression() else {
        return;
    };
    let expression = match argument.without_parentheses() {
        Expression::ArrowFunctionExpression(function)
            if !function.r#async
                && function.params.items.is_empty()
                && function.params.rest.is_none() =>
        {
            if function.expression {
                let Some(Statement::ExpressionStatement(statement)) =
                    function.body.statements.first()
                else {
                    return;
                };
                &statement.expression
            } else {
                let Some(expression) = single_return(&function.body.statements) else {
                    return;
                };
                expression
            }
        }
        Expression::FunctionExpression(function)
            if !function.r#async
                && !function.generator
                && function.params.items.is_empty()
                && function.params.rest.is_none() =>
        {
            let Some(body) = &function.body else { return };
            let Some(expression) = single_return(&body.statements) else {
                return;
            };
            expression
        }
        _ => return,
    };
    let expression_span = expression.span();
    let start = (offset + i64::from(expression_span.start)) as usize;
    let end = (offset + i64::from(expression_span.end)) as usize;
    let span = Span::new(
        (offset + i64::from(call.span.start)) as u32,
        (offset + i64::from(call.span.end)) as u32,
    );
    ctx.diagnostic_with_fix(
        "Unnecessary use of `$derived.by()`. Use `$derived()` directly for simple expressions.",
        span,
        Fix {
            span,
            replacement: format!("$derived({})", &ctx.source[start..end]),
        },
    );
}

fn single_return<'a, 'b>(statements: &'b [Statement<'a>]) -> Option<&'b Expression<'a>> {
    if statements.len() != 1 {
        return None;
    }
    let Statement::ReturnStatement(statement) = &statements[0] else {
        return None;
    };
    statement.argument.as_ref()
}

#[cfg(test)]
mod tests {
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn fixes_script_and_template_calls_at_original_unicode_offsets() {
        let source = "<!-- 😀 --><script data-note=\">\">const value = $derived.by(() => source /* keep */ + 1);</script><p>{$derived.by(function() { return value + 1; })}</p>";
        let alloc = Allocator::default();
        let parsed = parser::parse_for_lint(source, &alloc);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let diagnostics: Vec<_> = Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/prefer-derived-over-derived-by")
            .collect();
        assert_eq!(diagnostics.len(), 2);
        let mut output = source.to_string();
        for diagnostic in diagnostics.iter().rev() {
            let fix = diagnostic.fix.as_ref().unwrap();
            output.replace_range(
                fix.span.start as usize..fix.span.end as usize,
                &fix.replacement,
            );
        }
        assert_eq!(output, "<!-- 😀 --><script data-note=\">\">const value = $derived(source /* keep */ + 1);</script><p>{$derived(value + 1)}</p>");
    }

    #[test]
    fn retains_callbacks_with_effects_or_function_semantics() {
        let source = "<script lang=\"ts\">let value = $derived.by(() => { log(); return count; }); let second = $derived.by(async () => count); let third = $derived.by(function*() { return count; }); let fourth = $derived.by((count) => count); let fifth = $derived['by'](() => count); let sixth = $derived.by((() => count) as () => number);</script>";
        let alloc = Allocator::default();
        let parsed = parser::parse_for_lint(source, &alloc);
        assert!(parsed.errors.is_empty());
        assert!(Linter::all()
            .lint(&parsed.ast, source)
            .iter()
            .all(|d| d.rule_name != "svelte/prefer-derived-over-derived-by"));
        let source = "<svelte:options runes={false} /><script>let value = $derived.by(() => count);</script>";
        let parsed = parser::parse_for_lint(source, &alloc);
        assert!(Linter::all()
            .lint(&parsed.ast, source)
            .iter()
            .all(|d| d.rule_name != "svelte/prefer-derived-over-derived-by"));
    }
}
