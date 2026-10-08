//! Whitespace indentation with a shared token offset graph.
mod layout;
mod script;
mod template;
mod typescript;
use crate::linter::{LintContext, Rule};
use layout::{Layout, Options, RegionKind};
use oxc::{
    allocator::Allocator,
    ast::AstKind,
    parser::Parser,
    semantic::SemanticBuilder,
    span::{GetSpan, SourceType, Span},
};
pub struct Indent;
impl Rule for Indent {
    fn name(&self) -> &'static str {
        "svelte/indent"
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if !ctx
            .file_path
            .as_ref()
            .is_some_and(|p| p.ends_with(".svelte"))
        {
            return;
        }
        let mut layout = Layout::new(ctx.source, Options::parse(ctx.config.options.as_ref()));
        template::collect(ctx, &mut layout);
        let mut regions = Vec::new();
        for script in ctx.ast.instance.iter().chain(ctx.ast.module.iter()) {
            regions.push((script.content_span, String::new(), String::new(), true));
        }
        for r in &layout.regions {
            let (prefix, suffix) = match r.kind {
                RegionKind::Expression => ("void (", ");"),
                RegionKind::Const => ("const ", ";"),
                RegionKind::Binding => ("let ", " = null;"),
                RegionKind::Parameters => ("function _(", ") {}"),
            };
            regions.push((r.span, prefix.into(), suffix.into(), false));
        }
        for (span, prefix, suffix, _) in &regions {
            let text = format!(
                "{prefix}{}{suffix}",
                &ctx.source[span.start as usize..span.end as usize]
            );
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, &text, SourceType::tsx())
                .with_config(oxc::parser::config::TokensParserConfig)
                .parse();
            let base = span.start as i64 - prefix.len() as i64;
            for t in &parsed.tokens {
                let start = base + t.start() as i64;
                let end = base + t.end() as i64;
                if start >= span.start as i64 && end <= span.end as i64 && end > start {
                    layout.add_token(Span::new(start as u32, end as u32), false);
                }
            }
            for c in &parsed.program.comments {
                let start = base + c.span.start as i64;
                let end = base + c.span.end as i64;
                if start >= span.start as i64 && end <= span.end as i64 {
                    layout.add_token(Span::new(start as u32, end as u32), true);
                }
            }
        }
        layout.sort();
        template::apply(ctx, &mut layout);
        for (span, prefix, suffix, is_script) in regions {
            let text = format!(
                "{prefix}{}{suffix}",
                &ctx.source[span.start as usize..span.end as usize]
            );
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, &text, SourceType::tsx()).parse();
            let base = span.start as i64 - prefix.len() as i64;
            if is_script {
                if let Some(anchor) = layout.tokens.iter().rposition(|t| {
                    t.span.end <= span.start
                        && &layout.source[t.span.start as usize..t.span.end as usize] == "<"
                }) {
                    for statement in &parsed.program.body {
                        let s = statement.span();
                        if let Some(first) = layout.first(Span::new(
                            (base + s.start as i64) as u32,
                            (base + s.end as i64) as u32,
                        )) {
                            layout.set(first, i32::from(layout.options.indent_script), anchor);
                        }
                    }
                }
            }
            let semantic = SemanticBuilder::new().build(&parsed.program).semantic;
            for node in semantic.nodes().iter() {
                let kind = node.kind();
                let s = kind.span();
                let start = base + s.start as i64;
                let end = base + s.end as i64;
                if start < span.start as i64
                    || end > span.end as i64
                    || matches!(kind, AstKind::Program(_))
                {
                    continue;
                }
                let ancestors = semantic
                    .nodes()
                    .ancestors(node.id())
                    .map(|n| n.kind())
                    .collect::<Vec<_>>();

                let parent = ancestors.first().copied();
                script::apply_node(kind, parent, &ancestors, base, &mut layout);
                typescript::apply_node(kind, parent, base, &mut layout);
            }
        }
        layout.report(ctx);
    }
}
