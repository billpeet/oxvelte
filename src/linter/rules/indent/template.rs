//! Svelte markup and control-block offsets, matching the upstream indent visitor.
use super::layout::{Layout, Region, RegionKind};
use crate::ast::{Attribute, Fragment, TemplateNode};
use crate::linter::LintContext;
use oxc::span::Span;

fn span(node: &TemplateNode<'_>) -> Span {
    match node {
        TemplateNode::Text(n) => n.span,
        TemplateNode::Element(n) => n.span,
        TemplateNode::MustacheTag(n) => n.span,
        TemplateNode::RawMustacheTag(n) => n.span,
        TemplateNode::DebugTag(n) => n.span,
        TemplateNode::ConstTag(n) => n.span,
        TemplateNode::RenderTag(n) => n.span,
        TemplateNode::Comment(n) => n.span,
        TemplateNode::IfBlock(n) => n.span,
        TemplateNode::EachBlock(n) => n.span,
        TemplateNode::AwaitBlock(n) => n.span,
        TemplateNode::KeyBlock(n) => n.span,
        TemplateNode::SnippetBlock(n) => n.span,
    }
}

fn region(layout: &mut Layout<'_>, span: Span, kind: RegionKind) {
    if span.start < span.end {
        layout.regions.push(Region { span, kind });
    }
}

/// Add text tokens without decoding entities or losing source byte positions.
fn text_tokens(layout: &mut Layout<'_>, span: Span) {
    let source = layout.source;
    let mut start = None;
    for (offset, ch) in source[span.start as usize..span.end as usize].char_indices() {
        let pos = span.start + offset as u32;
        if ch.is_whitespace() {
            if let Some(begin) = start.take() {
                layout.add_token(Span::new(begin, pos), false);
            }
        } else if start.is_none() {
            start = Some(pos);
        }
    }
    if let Some(begin) = start {
        layout.add_token(Span::new(begin, span.end), false);
    }
}

/// Lex only markup. JavaScript expressions are tokenized by the shared engine.
fn markup(layout: &mut Layout<'_>, span: Span) {
    let source = layout.source;
    let mut pos = span.start;
    while pos < span.end {
        if let Some(end) = layout
            .regions
            .iter()
            .find(|r| r.span.start <= pos && pos < r.span.end)
            .map(|r| r.span.end)
        {
            pos = end;
            continue;
        }
        let ch = source[pos as usize..].chars().next().unwrap();
        if ch.is_whitespace() {
            pos += ch.len_utf8() as u32;
            continue;
        }
        let start = pos;
        if "<>/={}()[],\"'|".contains(ch) {
            pos += ch.len_utf8() as u32;
        } else {
            pos += ch.len_utf8() as u32;
            while pos < span.end {
                if layout.regions.iter().any(|r| r.span.start == pos) {
                    break;
                }
                let ch = source[pos as usize..].chars().next().unwrap();
                if ch.is_whitespace() || "<>/={}()[],\"'|".contains(ch) {
                    break;
                }
                pos += ch.len_utf8() as u32;
            }
        }
        layout.add_token(Span::new(start, pos), false);
    }
}

pub(super) fn collect(ctx: &LintContext<'_>, layout: &mut Layout<'_>) {
    collect_nodes(&ctx.ast.html.nodes, layout);
    for tag in &ctx.ast.html.template_tag_spans {
        markup(layout, tag.span);
    }
    for script in ctx.ast.instance.iter().chain(ctx.ast.module.iter()) {
        markup(
            layout,
            Span::new(script.span.start, script.content_span.start),
        );
        markup(layout, Span::new(script.content_span.end, script.span.end));
    }
    if let Some(style) = &ctx.ast.css {
        markup(
            layout,
            Span::new(style.span.start, style.content_span.start),
        );
        text_tokens(layout, style.content_span);
        markup(layout, Span::new(style.content_span.end, style.span.end));
    }
}

fn collect_nodes(nodes: &[TemplateNode<'_>], layout: &mut Layout<'_>) {
    for node in nodes {
        match node {
            TemplateNode::Text(n) => text_tokens(layout, n.span),
            TemplateNode::Comment(n) => layout.add_token(n.span, true),
            TemplateNode::Element(n) => {
                for meta in &n.attribute_meta {
                    if let Some(s) = meta.expression_span {
                        region(layout, s, RegionKind::Expression);
                    }
                    for part in &meta.parts {
                        if let Some(s) = part.expression_span {
                            region(layout, s, RegionKind::Expression);
                        }
                    }
                }
                markup(layout, Span::new(n.span.start, n.start_tag_end + 1));
                if let Some(s) = n.end_tag_span {
                    markup(layout, s);
                }
                collect_nodes(&n.children, layout);
            }
            TemplateNode::MustacheTag(n) => {
                region(layout, n.expression_span, RegionKind::Expression);
                markup(layout, n.span);
            }
            TemplateNode::RawMustacheTag(n) => {
                region(layout, n.expression_span, RegionKind::Expression);
                markup(layout, n.span);
            }
            TemplateNode::RenderTag(n) => {
                region(layout, n.expression_span, RegionKind::Expression);
                markup(layout, n.span);
            }
            TemplateNode::DebugTag(n) => {
                for s in &n.identifier_spans {
                    region(layout, *s, RegionKind::Expression);
                }
                markup(layout, n.span);
            }
            TemplateNode::ConstTag(n) => {
                region(layout, n.declaration_span, RegionKind::Const);
                markup(layout, n.span);
            }
            TemplateNode::IfBlock(n) => {
                region(layout, n.test_span, RegionKind::Expression);
                collect_nodes(&n.consequent.nodes, layout);
                if let Some(a) = &n.alternate {
                    collect_nodes(std::slice::from_ref(a.as_ref()), layout);
                }
            }
            TemplateNode::EachBlock(n) => {
                region(layout, n.context_span, RegionKind::Binding);
                for s in [Some(n.expression_span), n.index_span]
                    .into_iter()
                    .flatten()
                {
                    region(layout, s, RegionKind::Expression);
                }
                if let Some(key) = n.key_span {
                    let before = &layout.source[n.header_span.start as usize..key.start as usize];
                    let after = &layout.source[key.end as usize..n.header_span.end as usize];
                    let opening = before.trim_end();
                    let closing = after.trim_start();
                    let full = if opening.ends_with('(') && closing.starts_with(')') {
                        Span::new(
                            n.header_span.start + opening.len() as u32 - 1,
                            key.end + (after.len() - closing.len()) as u32 + 1,
                        )
                    } else {
                        key
                    };
                    region(layout, full, RegionKind::Expression);
                }
                collect_nodes(&n.body.nodes, layout);
                if let Some(f) = &n.fallback {
                    collect_nodes(&f.nodes, layout);
                }
            }
            TemplateNode::AwaitBlock(n) => {
                region(layout, n.expression_span, RegionKind::Expression);
                for s in [n.then_binding_span, n.catch_binding_span]
                    .into_iter()
                    .flatten()
                {
                    region(layout, s, RegionKind::Binding);
                }
                for f in [&n.pending, &n.then, &n.catch].into_iter().flatten() {
                    collect_nodes(&f.nodes, layout);
                }
            }
            TemplateNode::KeyBlock(n) => {
                region(layout, n.expression_span, RegionKind::Expression);
                collect_nodes(&n.body.nodes, layout);
            }
            TemplateNode::SnippetBlock(n) => {
                if let Some(params) = n.params_span {
                    region(layout, params, RegionKind::Parameters);
                }
                collect_nodes(&n.body.nodes, layout);
            }
        }
    }
}

pub(super) fn apply(ctx: &LintContext<'_>, layout: &mut Layout<'_>) {
    let roots: Vec<_> = ctx
        .ast
        .html
        .nodes
        .iter()
        .map(span)
        .chain(
            ctx.ast
                .instance
                .iter()
                .chain(ctx.ast.module.iter())
                .map(|s| s.span),
        )
        .chain(ctx.ast.css.iter().map(|s| s.span))
        .collect();
    for s in roots {
        if let Some(t) = layout.first(s) {
            layout.start(t, 0);
        }
    }
    apply_nodes(&ctx.ast.html.nodes, layout);
    for script in ctx.ast.instance.iter().chain(ctx.ast.module.iter()) {
        let open = Span::new(script.span.start, script.content_span.start);
        let close = Span::new(script.content_span.end, script.span.end);
        apply_start_tag(layout, open, &[]);
        apply_end_tag(layout, close);
        if let (Some(base), Some(end)) = (layout.first(open), layout.first(close)) {
            layout.set(end, 0, base);
            // Every top-level script statement is based on the opening tag;
            // the ES visitor subsequently assigns its internal offsets.
            if let Some(first) = layout.first(script.content_span) {
                layout.set(first, i32::from(layout.options.indent_script), base);
            }
        }
    }
    if let Some(style) = &ctx.ast.css {
        let open = Span::new(style.span.start, style.content_span.start);
        let close = Span::new(style.content_span.end, style.span.end);
        apply_start_tag(layout, open, &[]);
        apply_end_tag(layout, close);
        layout.ignore(style.content_span);
        if let (Some(base), Some(end)) = (layout.first(open), layout.first(close)) {
            layout.set(end, 0, base);
        }
    }
}

fn apply_start_tag(layout: &mut Layout<'_>, s: Span, attrs: &[Span]) {
    if let (Some(open), Some(close)) = (layout.first(s), layout.last(s)) {
        let align = layout.options.align_attributes_vertically;
        layout.list(
            attrs,
            layout.tokens[open].span,
            Some(layout.tokens[close].span),
            1,
            align,
        );
        if let Some(slash) = layout.before(close) {
            if layout.text(slash) == "/" {
                layout.set(slash, 0, open);
            }
        }
    }
}
fn apply_end_tag(layout: &mut Layout<'_>, s: Span) {
    if let (Some(open), Some(close)) = (layout.first(s), layout.last(s)) {
        layout.list(
            &[],
            layout.tokens[open].span,
            Some(layout.tokens[close].span),
            1,
            false,
        );
    }
}

fn children(layout: &mut Layout<'_>, nodes: &[TemplateNode<'_>], open: usize) {
    for child in nodes {
        if let Some(t) = layout.first(span(child)) {
            layout.set(t, 1, open);
        }
    }
}

fn apply_nodes(nodes: &[TemplateNode<'_>], layout: &mut Layout<'_>) {
    for (index, node) in nodes.iter().enumerate() {
        match node {
            TemplateNode::Text(n) => apply_text(layout, n.span, index == 0),
            TemplateNode::Comment(_) => {}
            TemplateNode::Element(n) => {
                let start = Span::new(n.span.start, n.start_tag_end + 1);
                let attrs: Vec<_> = n
                    .attributes
                    .iter()
                    .map(|a| match a {
                        Attribute::NormalAttribute { span, .. }
                        | Attribute::Directive { span, .. }
                        | Attribute::Spread { span } => *span,
                    })
                    .collect();
                if let Some(open) = layout.first(start) {
                    if let Some(end) = n.end_tag_span {
                        if let Some(t) = layout.first(end) {
                            layout.set(t, 0, open);
                        }
                        if !matches!(n.name.as_str(), "pre" | "textarea" | "template" | "style") {
                            let ns: Vec<_> = n.children.iter().filter(|n| !matches!(n, TemplateNode::Text(t) if t.data.trim().is_empty())).map(span).collect();
                            layout.list(&ns, start, Some(end), 1, false);
                        }
                    }
                }
                apply_start_tag(layout, start, &attrs);
                if let Some(end) = n.end_tag_span {
                    apply_end_tag(layout, end);
                }
                apply_attributes(n, layout);
                if matches!(n.name.as_str(), "pre" | "textarea" | "template" | "style") {
                    for child in &n.children {
                        layout.ignore(span(child));
                    }
                }
                apply_nodes(&n.children, layout);
            }
            TemplateNode::MustacheTag(n) => mustache(layout, n.span, &[n.expression_span]),
            TemplateNode::RawMustacheTag(n) => mustache(layout, n.span, &[n.expression_span]),
            TemplateNode::DebugTag(n) => mustache(layout, n.span, &n.identifier_spans),
            TemplateNode::ConstTag(n) => {
                if let (Some(open), Some(close)) = (layout.first(n.span), layout.last(n.span)) {
                    if let Some(t) = layout.after(open) {
                        layout.set(t, 1, open);
                    }
                    if let Some(t) = layout.first(n.declaration_span) {
                        layout.set(t, 1, open);
                    }
                    layout.set(close, 0, open);
                }
            }
            TemplateNode::RenderTag(n) => {
                if let Some(open) = layout.first(n.span) {
                    if let Some(render) = layout.after(open) {
                        layout.set(render, 1, open);
                        if let Some(t) = layout.first(n.expression_span) {
                            layout.set(t, 1, render);
                        }
                    }
                }
            }
            TemplateNode::IfBlock(n) => {
                block_header(layout, n.header_span, &[n.test_span]);
                if let Some(open) = layout.first(n.header_span) {
                    if n.elseif {
                        if let Some(else_token) = layout.after(open) {
                            if let Some(if_token) = layout.after(else_token) {
                                layout.set(if_token, 1, open);
                            }
                        }
                    }
                    children(layout, &n.consequent.nodes, open);
                    if let Some(a) = &n.alternate {
                        let branch_span = if let TemplateNode::IfBlock(branch) = a.as_ref() {
                            branch.header_span
                        } else {
                            span(a)
                        };
                        if let Some(t) = layout.first(branch_span) {
                            layout.set(t, 0, open);
                        }
                    }
                    if !n.elseif && !n.test.is_empty() {
                        block_close(layout, n.span, open);
                    }
                }
                apply_nodes(&n.consequent.nodes, layout);
                if let Some(a) = &n.alternate {
                    apply_nodes(std::slice::from_ref(a.as_ref()), layout);
                }
            }
            TemplateNode::EachBlock(n) => {
                let header = Span::new(n.span.start, n.body.span.start);
                let items: Vec<_> = [Some(n.expression_span), Some(n.context_span), n.index_span]
                    .into_iter()
                    .flatten()
                    .collect();
                block_header(layout, header, &items);
                if let Some(open) = layout.first(header) {
                    if let Some(key) = n.key_span {
                        if let Some((t, _)) = layout.first_last(key, header.start) {
                            if let Some(kw) = layout.after(open) {
                                layout.set(t, 1, kw);
                            }
                        }
                    }
                    children(layout, &n.body.nodes, open);
                    if let Some(f) = &n.fallback {
                        branch(layout, f, open, None);
                    }
                    block_close(layout, n.span, open);
                }
                apply_nodes(&n.body.nodes, layout);
                if let Some(f) = &n.fallback {
                    apply_nodes(&f.nodes, layout);
                }
            }
            TemplateNode::KeyBlock(n) => simple_block(layout, n.span, n.expression_span, &n.body),
            TemplateNode::SnippetBlock(n) => {
                let header = Span::new(n.span.start, n.body.span.start);
                block_header(layout, header, &[n.name_span]);
                if let Some(name) = layout.first(n.name_span) {
                    if let Some(left) = layout.after(name) {
                        if layout.text(left) == "(" {
                            layout.set(left, 1, name);
                            let right = layout
                                .tokens
                                .iter()
                                .enumerate()
                                .find(|(_, t)| {
                                    t.span.start
                                        >= n.params_span
                                            .map_or(layout.tokens[left].span.end, |s| s.end)
                                        && t.span.end <= header.end
                                        && &layout.source
                                            [t.span.start as usize..t.span.end as usize]
                                            == ")"
                                })
                                .map(|(i, _)| i);
                            if let Some(right) = right {
                                layout.list(
                                    &n.params_span.into_iter().collect::<Vec<_>>(),
                                    layout.tokens[left].span,
                                    Some(layout.tokens[right].span),
                                    1,
                                    false,
                                );
                            }
                        }
                    }
                }
                if let Some(open) = layout.first(header) {
                    children(layout, &n.body.nodes, open);
                    block_close(layout, n.span, open);
                }
                apply_nodes(&n.body.nodes, layout);
            }
            TemplateNode::AwaitBlock(n) => {
                let first_body = [&n.pending, &n.then, &n.catch]
                    .into_iter()
                    .flatten()
                    .map(|f| f.span.start)
                    .min()
                    .unwrap_or(n.span.end);
                let header = Span::new(n.span.start, first_body);
                block_header(layout, header, &[n.expression_span]);
                if let Some(open) = layout.first(header) {
                    if let Some(f) = &n.pending {
                        children(layout, &f.nodes, open);
                    }
                    if let Some(f) = &n.then {
                        branch(layout, f, open, n.then_binding_span);
                    }
                    if let Some(f) = &n.catch {
                        branch(layout, f, open, n.catch_binding_span);
                    }
                    block_close(layout, n.span, open);
                }
                for f in [&n.pending, &n.then, &n.catch].into_iter().flatten() {
                    apply_nodes(&f.nodes, layout);
                }
            }
        }
    }
}

fn apply_text(layout: &mut Layout<'_>, s: Span, first_child: bool) {
    if let Some(first) = layout.first(s) {
        let offset = if layout.is_beginning(layout.tokens[first].span) {
            0
        } else {
            i32::from(first_child)
        };
        let tokens: Vec<_> = layout
            .tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| s.start <= t.span.start && t.span.end <= s.end && !t.comment)
            .map(|(i, _)| i)
            .collect();
        for token in tokens {
            layout.set(token, offset, first);
        }
    }
}

fn apply_attributes(n: &crate::ast::Element<'_>, layout: &mut Layout<'_>) {
    for (attr, meta) in n.attributes.iter().zip(&n.attribute_meta) {
        let s = match attr {
            Attribute::NormalAttribute { span, .. }
            | Attribute::Directive { span, .. }
            | Attribute::Spread { span } => *span,
        };
        if let (Some(open), Some(close)) = (layout.first(s), layout.last(s)) {
            if layout.text(open) == "{" {
                layout.list(
                    &[],
                    layout.tokens[open].span,
                    Some(layout.tokens[close].span),
                    1,
                    false,
                );
                continue;
            }
            if let Some(eq_span) = meta.equals_span {
                if let Some(eq) = layout.first(eq_span) {
                    layout.set(eq, 1, open);
                    if let Some(value) = meta.value_full_span {
                        if let Some(v) = layout.first(value) {
                            layout.set(v, 1, open);
                            if matches!(layout.text(v), "\"" | "'" | "{") {
                                let values: Vec<_> = if matches!(
                                    attr,
                                    Attribute::NormalAttribute { .. }
                                        | Attribute::Directive {
                                            kind: crate::ast::DirectiveKind::StyleDirective,
                                            ..
                                        }
                                ) {
                                    if meta.parts.is_empty() {
                                        meta.mustache_span.or(meta.value_span).into_iter().collect()
                                    } else {
                                        meta.parts.iter().map(|p| p.span).collect()
                                    }
                                } else {
                                    Vec::new()
                                };
                                layout.list(
                                    &values,
                                    layout.tokens[v].span,
                                    Some(layout.tokens[close].span),
                                    1,
                                    false,
                                );
                            } else {
                                for part in &meta.parts {
                                    if let Some(t) = layout.first(part.span) {
                                        layout.set(t, 0, v);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for part in &meta.parts {
                if let (Some(m), Some(e)) = (part.mustache_span, part.expression_span) {
                    mustache(layout, m, &[e]);
                } else {
                    apply_text(layout, part.span, true);
                }
            }
            if meta.parts.is_empty() && meta.expression_span.is_none() {
                if let Some(value) = meta.value_span {
                    apply_text(layout, value, true);
                }
            }
            if let (Some(m), Some(e)) = (meta.mustache_span, meta.expression_span) {
                mustache(layout, m, &[e]);
            }
        }
    }
}

fn mustache(layout: &mut Layout<'_>, s: Span, exprs: &[Span]) {
    if let (Some(open), Some(close)) = (layout.first(s), layout.last(s)) {
        layout.list(
            exprs,
            layout.tokens[open].span,
            Some(layout.tokens[close].span),
            1,
            false,
        );
    }
}
fn block_header(layout: &mut Layout<'_>, s: Span, exprs: &[Span]) {
    if let (Some(open), Some(close)) = (layout.first(s), layout.last(s)) {
        if let Some(kw) = layout.after(open) {
            layout.set(kw, 1, open);
            layout.list(exprs, layout.tokens[kw].span, None, 1, false);
        }
        layout.set(close, 0, open);
    }
}
fn block_close(layout: &mut Layout<'_>, s: Span, open: usize) {
    if let Some(close) = layout.last(s) {
        if let Some(keyword) = layout.before(close) {
            if let Some(left) = layout.before(keyword) {
                if layout.text(left) == "{" && layout.text(keyword).starts_with('/') {
                    layout.set(left, 0, open);
                    layout.set(keyword, 1, left);
                    layout.set(close, 0, left);
                } else if layout.text(keyword) != "}" {
                    // The markup lexer separates `/` from a closing block name.
                    if let Some(slash) = layout.before(keyword) {
                        if layout.text(slash) == "/" {
                            if let Some(left) = layout.before(slash) {
                                layout.set(left, 0, open);
                                layout.set(slash, 1, left);
                                layout.set(keyword, 1, left);
                                layout.set(close, 0, left);
                            }
                        }
                    }
                }
            }
        }
    }
}
fn simple_block(layout: &mut Layout<'_>, s: Span, expression: Span, body: &Fragment<'_>) {
    let header = Span::new(s.start, body.span.start);
    block_header(layout, header, &[expression]);
    if let Some(open) = layout.first(header) {
        children(layout, &body.nodes, open);
        block_close(layout, s, open);
    }
    apply_nodes(&body.nodes, layout);
}
fn branch(layout: &mut Layout<'_>, f: &Fragment<'_>, parent: usize, binding: Option<Span>) {
    // Await continuation fragments include the header; each fallback fragments
    // begin after it. Handle both source shapes without scanning expression text.
    let direct = layout
        .first(f.span)
        .filter(|&i| layout.tokens[i].span.start == f.span.start && layout.text(i) == "{");
    let open = direct.or_else(|| {
        let last = layout
            .tokens
            .iter()
            .rposition(|t| t.span.end <= f.span.start)?;
        let mut depth = 0;
        for cursor in (0..=last).rev() {
            match layout.text(cursor) {
                "}" => depth += 1,
                "{" => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(cursor);
                    }
                }
                _ => {}
            }
        }
        None
    });
    if let Some(open) = open {
        if open != parent {
            layout.set(open, 0, parent);
        }
        if let Some(kw) = layout.after(open) {
            layout.set(kw, 1, open);
            if let Some(b) = binding {
                if let Some(t) = layout.first(b) {
                    let keyword = if open == parent {
                        layout.before(t).unwrap_or(kw)
                    } else {
                        kw
                    };
                    layout.set(keyword, 1, open);
                    layout.set(t, 1, keyword);
                }
            }
        }
        let mut depth = 0;
        for cursor in open..layout.tokens.len() {
            match layout.text(cursor) {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        layout.set(cursor, 0, open);
                        break;
                    }
                }
                _ => {}
            }
        }
        children(layout, &f.nodes, open);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collects_multiline_control_header_tokens() {
        let source = "<div>\n{\n#if\n ok\n}\ntext\n{\n/if\n}\n</div>";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        let ctx = LintContext::new(&parsed.ast, source);
        let mut layout = Layout::new(source, super::super::layout::Options::parse(None));
        collect(&ctx, &mut layout);
        layout.sort();
        let tokens: Vec<_> = (0..layout.tokens.len()).map(|i| layout.text(i)).collect();
        assert!(
            tokens.contains(&"#if"),
            "{tokens:?}; tags {:?}; nodes {:?}",
            parsed.ast.html.template_tag_spans,
            parsed.ast.html.nodes
        );
    }

    #[test]
    fn key_parentheses_and_preformatted_text_preserve_unicode_and_crlf() {
        use crate::linter::Rule;
        let source = "<!-- 😀 -->\r\n<div>\r\n{#each rows as row\r\n(\r\nrow\r\n.\r\nid\r\n)\r\n}\r\n<p>é</p>\r\n{/each}\r\n<pre>\r\nunchanged\r\n</pre>\r\n</div>";
        let expected = "<!-- 😀 -->\r\n<div>\r\n  {#each rows as row\r\n    (\r\n      row\r\n        .\r\n        id\r\n    )\r\n  }\r\n    <p>é</p>\r\n  {/each}\r\n  <pre>\r\nunchanged\r\n  </pre>\r\n</div>";
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let mut ctx = LintContext::new(&parsed.ast, source);
        ctx.file_path = Some("Component.svelte".into());
        super::super::Indent.run(&mut ctx);
        let mut fixes: Vec<_> = ctx
            .into_diagnostics()
            .into_iter()
            .filter_map(|d| d.fix)
            .collect();
        fixes.sort_by_key(|f| f.span.start);
        let mut result = source.to_owned();
        for fix in fixes.into_iter().rev() {
            result.replace_range(
                fix.span.start as usize..fix.span.end as usize,
                &fix.replacement,
            );
        }
        assert_eq!(result, expected);
    }
}
