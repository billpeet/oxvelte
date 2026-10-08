//! `svelte/no-unused-props` — disallow unused component props.
//! ⭐ Recommended

use crate::linter::{LintContext, Rule};
use oxc::ast::{ast::Expression, AstKind};
use oxc::span::{GetSpan, Span};
use std::collections::HashSet;

pub struct NoUnusedProps;

fn get_option_bool(options: &Option<serde_json::Value>, key: &str) -> bool {
    options
        .as_ref()
        .and_then(|o| o.as_array())
        .and_then(|a| a.first())
        .and_then(|o| o.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

impl Rule for NoUnusedProps {
    fn name(&self) -> &'static str {
        "svelte/no-unused-props"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let script = match &ctx.ast.instance {
            Some(s) => s,
            None => return,
        };
        if script.lang.as_deref() != Some("ts") {
            return;
        }
        let content = &script.content;

        let props_call = match content.find("$props()") {
            Some(pos) => pos,
            None => return,
        };
        // Upstream reports on the declarator's binding, including its type
        // annotation, for root properties, nested properties and index signatures.
        let Some(report_span) = props_binding_span(ctx, props_call as u32) else {
            return;
        };

        let before_props = &content[..props_call];
        let destructured = extract_destructured_props(before_props);
        let has_rest = before_props.contains("...");

        let decl_start = [before_props.rfind("let "), before_props.rfind("const ")]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(0);
        let decl = &before_props[decl_start..];
        let after_kw = decl.find('{');
        let uses_destructuring = after_kw.is_some() && {
            let brace_pos = after_kw.unwrap();
            let colon_pos = decl.find(':').unwrap_or(decl.len());
            brace_pos < colon_pos
        };

        if has_rest {
            return;
        }

        let check_imported_early = get_option_bool(&ctx.config.options, "checkImportedTypes");
        let resolve_path_early = if check_imported_early {
            ctx.file_path.as_deref()
        } else {
            None
        };

        if !uses_destructuring {
            let type_name = extract_type_name(before_props);
            let all_props = if let Some(ref tn) = &type_name {
                extract_type_properties_with_file(content, tn, resolve_path_early)
            } else {
                Vec::new()
            };

            if all_props.is_empty() {
                return;
            }

            let var_name = {
                let decl = &before_props[decl_start..];
                let after_kw = if decl.starts_with("let ") {
                    &decl[4..]
                } else if decl.starts_with("const ") {
                    &decl[6..]
                } else {
                    return;
                };
                let end = after_kw
                    .find(|c: char| c == ':' || c == '=' || c == ' ')
                    .unwrap_or(after_kw.len());
                after_kw[..end].trim()
            };
            if var_name.is_empty() {
                return;
            }

            let full_source = ctx.source;
            if full_source.contains(&format!("...{}", var_name))
                || full_source.contains(&format!("{{...{}}}", var_name))
            {
                return;
            }
            for (prop_name, _) in &all_props {
                if has_prop_access(full_source, var_name, prop_name) {
                    let allow_nested =
                        get_option_bool(&ctx.config.options, "allowUnusedNestedProperties");
                    if !allow_nested {
                        check_nested_properties(
                            content,
                            report_span,
                            full_source,
                            var_name,
                            prop_name,
                            ctx,
                        );
                    }
                    continue;
                }
                ctx.diagnostic(
                    format!("'{}' is an unused Props property.", prop_name),
                    report_span,
                );
            }
            return;
        }

        let type_name = extract_type_name(before_props);
        let ignore_patterns =
            extract_option_patterns(&ctx.config.options, "ignorePropertyPatterns");
        let ignore_type_patterns =
            extract_option_patterns(&ctx.config.options, "ignoreTypePatterns");
        let check_imported = get_option_bool(&ctx.config.options, "checkImportedTypes");

        let imported_file = ctx.file_path.clone();
        let resolve_path = if check_imported {
            imported_file.as_deref()
        } else {
            None
        };
        let all_props = if let Some(ref tn) = type_name {
            extract_type_properties_with_file(content, tn, resolve_path)
        } else {
            extract_inline_type_properties(before_props)
        };

        if all_props.is_empty() {
            return;
        }

        if !check_imported {
            if let Some(ref tn) = type_name {
                let is_directly_imported = content.contains(&format!("import {{ {}", tn))
                    || content.contains(&format!("import type {{ {}", tn))
                    || content.contains(&format!("import {{{}", tn));
                let is_locally_defined = content.contains(&format!("interface {}", tn))
                    || content.contains(&format!("type {} =", tn));
                if is_directly_imported && !is_locally_defined {
                    return;
                }
            }
        }

        let has_index_sig = type_name.as_ref().is_some_and(|tn| {
            [format!("interface {} ", tn), format!("type {} ", tn)]
                .iter()
                .any(|p| {
                    content
                        .find(p.as_str())
                        .and_then(|s| {
                            content[s..]
                                .find('{')
                                .map(|b| content[s + b..].contains("[key:"))
                        })
                        .unwrap_or(false)
                })
        });

        for (prop_name, prop_offset) in &all_props {
            if destructured.contains(prop_name.as_str()) {
                // Passing or spreading the object consumes all its properties.
                if ctx.source.contains(&format!("{{{}}}", prop_name))
                    || ctx.source.contains(&format!("...{}", prop_name))
                {
                    continue;
                }
                if !get_option_bool(&ctx.config.options, "allowUnusedNestedProperties") {
                    let nested = nested_type_properties(
                        content,
                        *prop_offset,
                        resolve_path,
                        &ignore_type_patterns,
                    );
                    for (sub_name, _) in nested {
                        if ignore_patterns
                            .iter()
                            .any(|p| matches_pattern(&sub_name, p))
                            || has_prop_access(ctx.source, prop_name, &sub_name)
                        {
                            continue;
                        }
                        ctx.diagnostic(
                            format!("'{}' in '{}' is an unused property.", sub_name, prop_name),
                            report_span,
                        );
                    }
                }
                continue;
            }

            if ignore_patterns
                .iter()
                .any(|p| matches_pattern(prop_name, p))
            {
                continue;
            }
            if let Some(ref tn) = type_name {
                if ignore_type_patterns.iter().any(|p| matches_pattern(tn, p)) {
                    continue;
                }
            }

            ctx.diagnostic(
                format!("'{}' is an unused Props property.", prop_name),
                report_span,
            );
        }
        if has_index_sig && !has_rest {
            ctx.diagnostic("Index signature is unused. Consider using rest operator (...) to capture remaining properties.", report_span);
        }
    }
}

fn nested_type_properties(
    content: &str,
    property_offset: usize,
    file_path: Option<&str>,
    ignore_types: &[String],
) -> Vec<(String, usize)> {
    let Some(rest) = content.get(property_offset..) else {
        return Vec::new();
    };
    let Some(colon) = rest.find(':') else {
        return Vec::new();
    };
    let rhs = rest[colon + 1..].trim_start();
    if rhs.starts_with('{') {
        let mut properties = Vec::new();
        let brace = property_offset + colon + 1 + rest[colon + 1..].find('{').unwrap();
        extract_props_from_block(content, brace, &mut properties);
        return properties;
    }
    let name = rhs
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        .next()
        .unwrap_or("");
    if name.is_empty() || ignore_types.iter().any(|p| matches_pattern(name, p)) {
        return Vec::new();
    }
    let local = extract_type_properties_with_file(content, name, file_path);
    if !local.is_empty() {
        return local;
    }
    file_path
        .map(|path| resolve_imported_type_properties(content, name, path))
        .unwrap_or_default()
}

fn props_binding_span(ctx: &LintContext<'_>, call_start: u32) -> Option<Span> {
    let semantic = ctx.instance_semantic?;
    let content_offset = ctx.ast.instance.as_ref()?.content_span.start;
    semantic.nodes().iter().find_map(|node| {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return None;
        };
        let Some(Expression::CallExpression(call)) = &decl.init else {
            return None;
        };
        if !matches!(&call.callee, Expression::Identifier(id) if id.name == "$props" && id.span.start == call_start) {
            return None;
        }
        let binding = decl.id.span();
        let end = decl.type_annotation.as_ref().map_or(binding.end, |annotation| annotation.span.end);
        Some(Span::new(content_offset + binding.start, content_offset + end))
    })
}

fn has_prop_access(source: &str, base: &str, prop: &str) -> bool {
    source.contains(&format!("{}.{}", base, prop))
        || source.contains(&format!("{}['{}']", base, prop))
        || source.contains(&format!("{}[\"{}\"]", base, prop))
}

fn extract_destructured_props(before_props: &str) -> HashSet<String> {
    let mut props = HashSet::new();
    let decl_start = [before_props.rfind("let "), before_props.rfind("const ")]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0);
    let after_decl = &before_props[decl_start..];
    let open = match after_decl.find('{') {
        Some(p) => decl_start + p,
        None => return props,
    };
    let mut depth = 0;
    let mut close = None;
    for (i, b) in before_props[open..].bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    if let Some(close) = close {
        if open < close {
            let inner = &before_props[open + 1..close];
            let parts = split_at_depth0(inner, ',');
            for part in &parts {
                let part = part
                    .lines()
                    .filter(|l| !l.trim().starts_with("//"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let part = part.trim();
                if part.starts_with("...") {
                    continue;
                }
                let mut name_end = part.len();
                let mut d = 0i32;
                let pbytes = part.as_bytes();
                for (i, c) in part.char_indices() {
                    match c {
                        '{' | '(' | '[' | '<' => d += 1,
                        '}' | ')' | ']' => {
                            d -= 1;
                            if d < 0 {
                                d = 0;
                            }
                        }
                        '>' => {
                            if !(i > 0 && pbytes[i - 1] == b'=') {
                                d -= 1;
                                if d < 0 {
                                    d = 0;
                                }
                            }
                        }
                        ':' | '=' if d == 0 => {
                            name_end = i;
                            break;
                        }
                        _ => {}
                    }
                }
                let name = part[..name_end].trim().trim_matches('\'').trim_matches('"');
                if !name.is_empty() {
                    props.insert(name.to_string());
                }
            }
        }
    }
    props
}

fn split_at_depth0(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let bytes = s.as_bytes();
    for (i, c) in s.char_indices() {
        match c {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if depth < 0 {
                    depth = 0;
                }
            }
            '>' => {
                if !(i > 0 && bytes[i - 1] == b'=') {
                    depth -= 1;
                    if depth < 0 {
                        depth = 0;
                    }
                }
            }
            c if c == sep && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

fn extract_type_name(before_props: &str) -> Option<String> {
    let before_eq = before_props.trim_end().strip_suffix('=')?.trim_end();
    let mut depth = 0i32;
    let mut last_colon = None;
    for (i, c) in before_eq.char_indices() {
        match c {
            '{' | '(' => depth += 1,
            '}' | ')' => {
                depth -= 1;
                if depth < 0 {
                    depth = 0;
                }
            }
            ':' if depth == 0 => last_colon = Some(i),
            _ => {}
        }
    }
    let colon_pos = last_colon?;
    let after_colon = before_eq[colon_pos + 1..].trim();
    if after_colon.starts_with('{') {
        return None;
    }
    let name = after_colon
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        .next()?;
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn extract_type_properties_with_file(
    content: &str,
    type_name: &str,
    file_path: Option<&str>,
) -> Vec<(String, usize)> {
    let mut props = Vec::new();
    let iface_patterns = [
        format!("interface {} ", type_name),
        format!("interface {} {{", type_name),
    ];
    let iface_start = iface_patterns
        .iter()
        .filter_map(|p| content.find(p.as_str()))
        .min();

    if let Some(start) = iface_start {
        if let Some(brace_rel) = content[start..].find('{') {
            let before_brace = &content[start..start + brace_rel];
            if let Some(ext_pos) = before_brace.find("extends ") {
                for base in before_brace[ext_pos + 8..].trim().split(',') {
                    let bn = base.trim();
                    if bn.is_empty() {
                        continue;
                    }
                    let bp = extract_type_properties_with_file(content, bn, None);
                    if !bp.is_empty() {
                        props.extend(bp);
                    } else if let Some(fp) = file_path {
                        props.extend(resolve_imported_type_properties(content, bn, fp));
                    }
                }
            }
            extract_props_from_block(content, start + brace_rel, &mut props);
        }
        return props;
    }

    let type_start = content.find(&format!("type {} =", type_name));

    if let Some(start) = type_start {
        let eq_pos = content[start..].find('=').unwrap_or(0);
        let rhs_start = start + eq_pos + 1;
        let rhs = content[rhs_start..].trim_start();

        if rhs.contains('&') {
            for part in &split_at_depth0(&rhs[..find_type_end(rhs)], '&') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                if part.starts_with('{') {
                    extract_props_from_block(
                        content,
                        rhs_start + content[rhs_start..].find(part).unwrap_or(0),
                        &mut props,
                    );
                } else {
                    let rn = part
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("");
                    if !rn.is_empty() && rn != type_name {
                        props.extend(extract_type_properties_with_file(content, rn, file_path));
                    }
                }
            }
        } else if let Some(brace_rel) = content[start..].find('{') {
            extract_props_from_block(content, start + brace_rel, &mut props);
        }
    }
    props
}

fn find_type_end(s: &str) -> usize {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '{' | '(' | '<' => depth += 1,
            '}' | ')' | '>' => {
                depth -= 1;
                if depth < 0 {
                    return i;
                }
            }
            ';' if depth == 0 => return i,
            _ => {}
        }
    }
    s.len()
}

fn resolve_imported_type_properties(
    content: &str,
    type_name: &str,
    file_path: &str,
) -> Vec<(String, usize)> {
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("import") || !trimmed.contains(type_name) {
            continue;
        }
        let Some(from_pos) = trimmed.find("from ") else {
            continue;
        };
        let module = trimmed[from_pos + 5..]
            .trim()
            .trim_end_matches(';')
            .trim_matches('\'')
            .trim_matches('"');
        if !module.starts_with('.') {
            continue;
        }
        let dir = std::path::Path::new(file_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        for ext in &["", ".ts", ".d.ts"] {
            if let Ok(imported_content) =
                std::fs::read_to_string(dir.join(format!("{}{}", module, ext)))
            {
                let base_props =
                    extract_type_properties_with_file(&imported_content, type_name, None);
                if !base_props.is_empty() {
                    let offset = content
                        .find(&format!("extends {}", type_name))
                        .or_else(|| content.find(type_name))
                        .unwrap_or(0);
                    return base_props
                        .into_iter()
                        .map(|(name, _)| (name, offset))
                        .collect();
                }
            }
        }
    }
    Vec::new()
}

fn extract_inline_type_properties(before_props: &str) -> Vec<(String, usize)> {
    let mut props = Vec::new();
    let before_eq = match before_props.trim_end().strip_suffix('=') {
        Some(s) => s.trim_end(),
        None => return props,
    };
    if let Some(close) = before_eq.rfind('}') {
        let mut depth = 0;
        let mut open = None;
        for i in (0..=close).rev() {
            match before_eq.as_bytes()[i] {
                b'}' => depth += 1,
                b'{' => {
                    depth -= 1;
                    if depth == 0 {
                        open = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(brace_pos) = open {
            let before_brace = before_eq[..brace_pos].trim_end();
            if before_brace.ends_with(':') {
                extract_props_from_block(before_eq, brace_pos, &mut props);
            }
        }
    }
    props
}

fn extract_props_from_block(content: &str, brace_start: usize, props: &mut Vec<(String, usize)>) {
    let after = &content[brace_start + 1..];
    let mut depth = 1;
    let mut end = after.len();
    for (i, b) in after.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let block = &after[..end];

    let stripped = strip_block_comments(block);
    let block_ref = stripped.as_str();
    let mut depth = 0i32;
    let mut line_start = 0;
    let block_bytes = block_ref.as_bytes();
    for (i, b) in block_ref.bytes().enumerate() {
        match b {
            b'{' | b'(' | b'<' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth -= 1;
                if depth < 0 {
                    depth = 0;
                }
            }
            b'>' => {
                if !(i > 0 && block_bytes[i - 1] == b'=') {
                    depth -= 1;
                    if depth < 0 {
                        depth = 0;
                    }
                }
            }
            b';' | b'\n' if depth == 0 => {
                let segment = &block_ref[line_start..i];
                let trimmed = segment.trim();
                line_start = i + 1;
                if trimmed.is_empty() || trimmed.starts_with("//") {
                    continue;
                }
                if trimmed.starts_with('[') {
                    continue;
                }

                let name = if trimmed.starts_with('\'') || trimmed.starts_with('"') {
                    let q = trimmed.as_bytes()[0] as char;
                    trimmed[1..].find(q).map(|end| &trimmed[1..end + 1])
                } else {
                    let end = trimmed
                        .find(|c: char| c == ':' || c == '?' || c == '(' || c == '<')
                        .unwrap_or(trimmed.len());
                    Some(trimmed[..end].trim())
                };
                if let Some(name) = name {
                    let name = name.trim();
                    if name.is_empty() || name.starts_with("//") || name.starts_with('*') {
                        continue;
                    }
                    let offset = block.find(name).map(|p| brace_start + 1 + p).unwrap_or(0);
                    props.push((name.to_string(), offset));
                }
            }
            _ => {}
        }
    }
}

fn strip_block_comments(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    result.push('\n');
                }
                i += 1;
            }
            if i + 1 < bytes.len() {
                i += 2;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn extract_option_patterns(options: &Option<serde_json::Value>, key: &str) -> Vec<String> {
    options
        .as_ref()
        .and_then(|o| o.as_array())
        .and_then(|a| a.first())
        .and_then(|o| o.get(key))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn check_nested_properties(
    content: &str,
    report_span: Span,
    full_source: &str,
    var_name: &str,
    prop_name: &str,
    ctx: &mut LintContext<'_>,
) {
    let Some(pos) = content
        .find(&format!("{}: {{", prop_name))
        .or_else(|| content.find(&format!("{}?: {{", prop_name)))
    else {
        return;
    };
    let Some(bs) = content[pos..].find('{').map(|p| pos + p) else {
        return;
    };
    let mut nested = Vec::new();
    extract_props_from_block(content, bs, &mut nested);
    if nested.is_empty() {
        return;
    }
    let prefix = format!("{}.{}", var_name, prop_name);
    for (sub_name, _) in &nested {
        if has_prop_access(full_source, &prefix, sub_name) {
            continue;
        }
        ctx.diagnostic(
            format!("'{}' in '{}' is an unused property.", sub_name, prop_name),
            report_span,
        );
    }
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
    if pattern.starts_with('/') {
        let inner = pattern.trim_start_matches('/');
        let inner = inner.rsplit_once('/').map(|(p, _)| p).unwrap_or(inner);
        if inner.starts_with('^') {
            let after_caret = &inner[1..];
            if after_caret.starts_with('[') {
                if let Some(close) = after_caret.find(']') {
                    let chars = &after_caret[1..close];
                    return name.starts_with(|c: char| chars.contains(c));
                }
            }
            if after_caret.starts_with('(') {
                let alts = after_caret.trim_start_matches('(').trim_end_matches(')');
                return alts.split('|').any(|alt| name.starts_with(alt));
            }
            return name.starts_with(after_caret);
        }
        name.contains(inner)
    } else {
        name == pattern
    }
}

#[cfg(test)]
mod tests {
    use crate::linter::{LintDiagnostic, Linter};
    use crate::parser;
    use oxc::allocator::Allocator;

    #[test]
    fn destructured_named_types_respect_nested_options() {
        let source = r#"<script lang="ts">
            interface Details { used: string; hidden: string; _internal: string; }
            interface Props { details: Details; ignored: Details; unused: string; }
            let { details, ignored }: Props = $props();
            console.log(details.used);
        </script>"#;
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        for (options, expected) in [
            (
                serde_json::json!([{"ignorePropertyPatterns": ["/^_/"], "ignoreTypePatterns": ["Details"]}]),
                vec!["'unused' is an unused Props property."],
            ),
            (
                serde_json::json!([{"ignorePropertyPatterns": ["/^_/"]}]),
                vec![
                    "'hidden' in 'details' is an unused property.",
                    "'used' in 'ignored' is an unused property.",
                    "'hidden' in 'ignored' is an unused property.",
                    "'unused' is an unused Props property.",
                ],
            ),
            (
                serde_json::json!([{"allowUnusedNestedProperties": true}]),
                vec!["'unused' is an unused Props property."],
            ),
        ] {
            let diagnostics = Linter::all().lint_with_config(
                &parsed.ast,
                source,
                crate::linter::RuleConfig {
                    options: Some(options),
                    ..Default::default()
                },
            );
            assert_eq!(
                diagnostics
                    .iter()
                    .filter(|d| d.rule_name == "svelte/no-unused-props")
                    .map(|d| d.message.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    fn lint(source: &str) -> Vec<LintDiagnostic> {
        let allocator = Allocator::default();
        let result = parser::parse(source, &allocator);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        Linter::all()
            .lint(&result.ast, source)
            .into_iter()
            .filter(|diag| diag.rule_name == "svelte/no-unused-props")
            .collect()
    }

    #[test]
    fn root_and_nested_findings_use_the_typed_props_binding_span() {
        let source = r#"<!-- 😀 -->
<script lang="ts" data-note=">">
    interface Props {
        unused: string;
        user: { used: string; hidden: number; };
    }
    const props: Props = $props();
    console.log(props.user.used);
</script>"#;
        let diags = lint(source);
        assert_eq!(
            diags
                .iter()
                .map(|diag| diag.message.as_str())
                .collect::<Vec<_>>(),
            [
                "'unused' is an unused Props property.",
                "'hidden' in 'user' is an unused property."
            ]
        );
        let start = source.find("props: Props").unwrap();
        for diag in diags {
            assert_eq!(diag.span.start as usize, start);
            assert_eq!(
                &source[diag.span.start as usize..diag.span.end as usize],
                "props: Props"
            );
        }
    }

    #[test]
    fn unused_properties_precede_index_signature_at_the_same_binding() {
        let source = r#"<script lang="ts">
    interface Props {
        used: string;
        unused: number;
        [key: string]: unknown;
    }
    let /* binding */ { used }: Props = $props();
</script>"#;
        let diags = lint(source);
        assert_eq!(
            diags.iter().map(|diag| diag.message.as_str()).collect::<Vec<_>>(),
            ["'unused' is an unused Props property.", "Index signature is unused. Consider using rest operator (...) to capture remaining properties."]
        );
        for diag in diags {
            assert_eq!(
                &source[diag.span.start as usize..diag.span.end as usize],
                "{ used }: Props"
            );
        }
    }
}
