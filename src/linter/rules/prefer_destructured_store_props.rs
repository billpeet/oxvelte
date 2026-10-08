//! `svelte/prefer-destructured-store-props` offers reactive destructuring alternatives.
use crate::ast::{Attribute, AttributeValue, AttributeValuePart, Fragment, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule, Suggestion};
use oxc::ast::ast::{
    AssignmentTarget, AssignmentTargetMaybeDefault, AssignmentTargetProperty, Expression,
    PropertyKey,
};
use oxc::ast::AstKind;
use oxc::span::{GetSpan, Span};
use std::collections::{HashMap, HashSet};
pub struct PreferDestructuredStoreProps;
impl Rule for PreferDestructuredStoreProps {
    fn name(&self) -> &'static str {
        "svelte/prefer-destructured-store-props"
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let mut reactive: HashMap<(String, String), Vec<String>> = HashMap::new();
        let mut top_names = HashSet::new();
        for semantic in [ctx.instance_semantic, ctx.module_semantic]
            .into_iter()
            .flatten()
        {
            let scoping = semantic.scoping();
            top_names.extend(
                scoping
                    .get_bindings(scoping.root_scope_id())
                    .keys()
                    .map(|n| n.to_string()),
            );
        }
        if let Some(semantic) = ctx.instance_semantic {
            let nodes = semantic.nodes();
            for node in nodes.iter() {
                let AstKind::AssignmentExpression(assignment) = node.kind() else {
                    continue;
                };
                let mut parent = nodes.parent_id(node.id());
                while matches!(nodes.kind(parent), AstKind::ParenthesizedExpression(_)) {
                    parent = nodes.parent_id(parent);
                }
                if !matches!(nodes.kind(parent), AstKind::ExpressionStatement(_)) {
                    continue;
                }
                if !matches!(nodes.parent_kind(parent), AstKind::LabeledStatement(label) if label.label.name == "$")
                {
                    continue;
                }
                let label = nodes.parent_id(parent);
                if !matches!(nodes.parent_kind(label), AstKind::Program(_)) {
                    continue;
                }
                match (&assignment.left, &assignment.right) {
                    (
                        AssignmentTarget::AssignmentTargetIdentifier(target),
                        Expression::StaticMemberExpression(member),
                    ) => {
                        if let Expression::Identifier(store) = &member.object {
                            top_names.insert(target.name.to_string());
                            reactive
                                .entry((store.name.to_string(), member.property.name.to_string()))
                                .or_default()
                                .push(target.name.to_string());
                        }
                    }
                    (
                        AssignmentTarget::ObjectAssignmentTarget(pattern),
                        Expression::Identifier(store),
                    ) => {
                        for property in &pattern.properties {
                            let pair = match property {
                                AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(
                                    property,
                                ) if property.init.is_none() => Some((
                                    property.binding.name.to_string(),
                                    property.binding.name.to_string(),
                                )),
                                AssignmentTargetProperty::AssignmentTargetPropertyProperty(
                                    property,
                                ) => {
                                    let name = match &property.name {
                                        PropertyKey::StaticIdentifier(id) => {
                                            Some(id.name.to_string())
                                        }
                                        PropertyKey::StringLiteral(lit) => {
                                            Some(lit.value.to_string())
                                        }
                                        _ => None,
                                    };
                                    match (name, &property.binding) {
                                        (Some(name), AssignmentTargetMaybeDefault::AssignmentTargetIdentifier(id)) => Some((name, id.name.to_string())),
                                        _ => None,
                                    }
                                }
                                _ => None,
                            };
                            if let Some((property, variable)) = pair {
                                top_names.insert(variable.clone());
                                reactive
                                    .entry((store.name.to_string(), property))
                                    .or_default()
                                    .push(variable);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut local_regions = Vec::new();
        walk_template_nodes(&ctx.ast.html, &mut |node| match node {
            TemplateNode::EachBlock(block) => {
                local_regions.push((block.body.span, binding_names(&block.context)));
                if let Some(index) = &block.index {
                    local_regions.push((block.body.span, vec![index.clone()]));
                }
            }
            TemplateNode::AwaitBlock(block) => {
                if let (Some(body), Some(binding)) = (&block.then, &block.then_binding) {
                    local_regions.push((body.span, binding_names(binding)));
                }
                if let (Some(body), Some(binding)) = (&block.catch, &block.catch_binding) {
                    local_regions.push((body.span, binding_names(binding)));
                }
            }
            TemplateNode::Element(el) => {
                for attr in &el.attributes {
                    if let Attribute::Directive {
                        kind: crate::ast::DirectiveKind::Let,
                        name,
                        value,
                        ..
                    } = attr
                    {
                        let names = match value {
                            AttributeValue::Expression(pattern) => binding_names(pattern),
                            _ => vec![name.clone()],
                        };
                        local_regions.push((Span::new(el.start_tag_end + 1, el.span.end), names));
                    }
                }
            }
            TemplateNode::SnippetBlock(block) => {
                local_regions.push((block.body.span, binding_names(&block.params)))
            }
            _ => {}
        });
        let mut containers = vec![ctx.ast.html.span];
        walk_template_nodes(&ctx.ast.html, &mut |node| match node {
            TemplateNode::Element(el) => {
                containers.push(Span::new(el.start_tag_end + 1, el.span.end))
            }
            TemplateNode::EachBlock(block) => {
                containers.push(block.body.span);
                if let Some(fallback) = &block.fallback {
                    containers.push(fallback.span);
                }
            }
            TemplateNode::IfBlock(block) => {
                containers.push(block.consequent.span);
            }
            TemplateNode::AwaitBlock(block) => {
                for body in [&block.pending, &block.then, &block.catch]
                    .into_iter()
                    .flatten()
                {
                    containers.push(body.span);
                }
            }
            TemplateNode::KeyBlock(block) => containers.push(block.body.span),
            TemplateNode::SnippetBlock(block) => containers.push(block.body.span),
            _ => {}
        });
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            if let TemplateNode::ConstTag(tag) = node {
                let scope = containers
                    .iter()
                    .filter(|span| span.start <= tag.span.start && span.end >= tag.span.end)
                    .min_by_key(|span| span.end - span.start)
                    .unwrap();
                let allocator = oxc::allocator::Allocator::default();
                let declaration = format!("const {};", tag.declaration);
                let parsed =
                    oxc::parser::Parser::new(&allocator, &declaration, oxc::span::SourceType::ts())
                        .parse();
                if parsed.errors.is_empty() {
                    let semantic = oxc::semantic::SemanticBuilder::new()
                        .build(&parsed.program)
                        .semantic;
                    let names = semantic
                        .scoping()
                        .get_bindings(semantic.scoping().root_scope_id())
                        .keys()
                        .map(|name| name.to_string())
                        .collect();
                    local_regions.push((Span::new(tag.span.end, scope.end), names));
                }
            }
        });
        visit_template_expressions(&ctx.ast.html, ctx.source, &mut |text, expression_span| {
            let allocator = oxc::allocator::Allocator::default();
            let wrapper = format!("({text});");
            let parsed =
                oxc::parser::Parser::new(&allocator, &wrapper, oxc::span::SourceType::ts()).parse();
            if !parsed.errors.is_empty() {
                return;
            }
            let semantic = oxc::semantic::SemanticBuilder::new()
                .build(&parsed.program)
                .semantic;
            let nodes = semantic.nodes();
            for node in nodes.iter() {
                let (object, property, computed, member_span) = match node.kind() {
                    AstKind::StaticMemberExpression(member) => (
                        &member.object,
                        member.property.name.to_string(),
                        false,
                        member.span,
                    ),
                    AstKind::ComputedMemberExpression(member) => (
                        &member.object,
                        wrapper[member.expression.span().start as usize
                            ..member.expression.span().end as usize]
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" "),
                        true,
                        member.span,
                    ),
                    _ => continue,
                };
                let Expression::Identifier(store) = object else {
                    continue;
                };
                if !store.name.starts_with('$')
                    || store.name.starts_with("$$")
                    || store.name.len() < 2
                {
                    continue;
                }
                if ctx.is_runes
                    && matches!(
                        store.name.as_str(),
                        "$state"
                            | "$derived"
                            | "$effect"
                            | "$props"
                            | "$inspect"
                            | "$bindable"
                            | "$host"
                    )
                {
                    continue;
                }
                let local = nodes.iter().any(|id_node| {
                    let AstKind::IdentifierReference(id) = id_node.kind() else {
                        return false;
                    };
                    if id.span.start < member_span.start || id.span.end > member_span.end {
                        return false;
                    }
                    semantic
                        .scoping()
                        .get_reference(id.reference_id())
                        .symbol_id()
                        .is_some()
                        || local_regions.iter().any(|(span, names)| {
                            span.start <= expression_span.start
                                && span.end >= expression_span.end
                                && names.iter().any(|name| name == id.name.as_str())
                        })
                });
                if local {
                    continue;
                }
                let span = Span::new(
                    expression_span.start + member_span.start - 1,
                    expression_span.start + member_span.end - 1,
                );
                let mut active_names: HashSet<String> = local_regions
                    .iter()
                    .filter(|(region, _)| {
                        region.start <= expression_span.start && region.end >= expression_span.end
                    })
                    .flat_map(|(_, names)| names.iter().cloned())
                    .collect();
                for scope in semantic.scoping().scope_ancestors(node.scope_id()) {
                    active_names.extend(
                        semantic
                            .scoping()
                            .get_bindings(scope)
                            .keys()
                            .map(|name| name.to_string()),
                    );
                }
                let mut suggestions = Vec::new();
                if !computed {
                    if let Some(variables) =
                        reactive.get(&(store.name.to_string(), property.clone()))
                    {
                        let mut seen = HashSet::new();
                        for variable in variables {
                            if !active_names.contains(variable) && seen.insert(variable) {
                                suggestions.push(Suggestion {
                                    description: format!(
                                        "Using the predefined reactive variable {variable}"
                                    ),
                                    fix: Fix {
                                        span,
                                        replacement: variable.clone(),
                                    },
                                });
                            }
                        }
                    }
                    if let Some(script) = &ctx.ast.instance {
                        let insert = script.content_span.end;
                        let base = property.strip_prefix('$').unwrap_or(&property);
                        let mut variable = base.to_string();
                        let mut suffix = 0;
                        if reserved(&variable)
                            || matches!(
                                variable.as_str(),
                                "undefined"
                                    | "NaN"
                                    | "Infinity"
                                    | "Object"
                                    | "Array"
                                    | "String"
                                    | "Number"
                                    | "Boolean"
                                    | "Symbol"
                                    | "BigInt"
                                    | "Date"
                                    | "Map"
                                    | "Set"
                                    | "Math"
                                    | "JSON"
                                    | "Promise"
                                    | "RegExp"
                                    | "Error"
                                    | "console"
                            )
                        {
                            suffix += 1;
                            variable = format!("{base}{suffix}");
                        }
                        while top_names.contains(&variable) || active_names.contains(&variable) {
                            suffix += 1;
                            variable = format!("{base}{suffix}");
                        }
                        let alias = if variable == property {
                            String::new()
                        } else {
                            format!(": {variable}")
                        };
                        let declaration =
                            format!("$: ({{ {property}{alias} }} = {});\n", store.name);
                        let fix = merge_edits(
                            ctx.source,
                            vec![(Span::new(insert, insert), declaration), (span, variable)],
                        );
                        suggestions.push(Suggestion { description: format!("Using destructuring like $: ({{ {property} }} = {}); will run faster", store.name), fix });
                    }
                }
                ctx.diagnostic_with_suggestions(
                    format!(
                        "Destructure {property} from {} for better change tracking & fewer redraws",
                        store.name
                    ),
                    span,
                    suggestions,
                );
            }
        });
    }
}
fn merge_edits(source: &str, mut edits: Vec<(Span, String)>) -> Fix {
    edits.sort_by_key(|(span, _)| span.start);
    let start = edits[0].0.start;
    let end = edits.last().unwrap().0.end;
    let mut replacement = String::new();
    let mut cursor = start;
    for (span, text) in edits {
        replacement.push_str(&source[cursor as usize..span.start as usize]);
        replacement.push_str(&text);
        cursor = span.end;
    }
    Fix {
        span: Span::new(start, end),
        replacement,
    }
}
fn reserved(name: &str) -> bool {
    matches!(
        name,
        "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "instanceof"
            | "interface"
            | "let"
            | "new"
            | "null"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "static"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "arguments"
            | "eval"
    )
}
fn binding_names(pattern: &str) -> Vec<String> {
    let allocator = oxc::allocator::Allocator::default();
    let wrapper = format!("({pattern})=>0");
    let parsed =
        oxc::parser::Parser::new(&allocator, &wrapper, oxc::span::SourceType::ts()).parse();
    if !parsed.errors.is_empty() {
        return Vec::new();
    }
    let semantic = oxc::semantic::SemanticBuilder::new()
        .build(&parsed.program)
        .semantic;
    semantic
        .nodes()
        .iter()
        .filter_map(|n| match n.kind() {
            AstKind::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
        })
        .collect()
}
fn visit_template_expressions(
    fragment: &Fragment,
    source: &str,
    visitor: &mut impl FnMut(&str, Span),
) {
    walk_template_nodes(fragment, &mut |node| match node {
        TemplateNode::Element(el) => {
            for (idx, attr) in el.attributes.iter().enumerate() {
                match attr {
                    Attribute::NormalAttribute { value, .. }
                    | Attribute::Directive { value, .. } => match value {
                        AttributeValue::Expression(text) => {
                            if let Some(span) =
                                el.attribute_meta.get(idx).and_then(|m| m.expression_span)
                            {
                                visitor(text.as_str(), span);
                            }
                        }
                        AttributeValue::Concat(parts) => {
                            for (part_idx, part) in parts.iter().enumerate() {
                                let AttributeValuePart::Expression(text) = part else {
                                    continue;
                                };
                                let Some(span) = el
                                    .attribute_meta
                                    .get(idx)
                                    .and_then(|m| m.parts.get(part_idx))
                                    .and_then(|p| p.expression_span)
                                else {
                                    continue;
                                };
                                visitor(text.as_str(), span);
                            }
                        }
                        _ => {}
                    },
                    Attribute::Spread { span } => {
                        if let Some(expression_span) = el
                            .attribute_meta
                            .get(idx)
                            .and_then(|meta| meta.expression_span)
                        {
                            visitor(
                                &source
                                    [expression_span.start as usize..expression_span.end as usize],
                                expression_span,
                            );
                        } else {
                            let expression_span = Span::new(span.start + 4, span.end - 1);
                            visitor(
                                &source
                                    [expression_span.start as usize..expression_span.end as usize],
                                expression_span,
                            );
                        }
                    }
                }
            }
        }
        TemplateNode::MustacheTag(tag) => visitor(tag.expression.as_str(), tag.expression_span),
        TemplateNode::RawMustacheTag(tag) => visitor(tag.expression.as_str(), tag.expression_span),
        TemplateNode::RenderTag(tag) => visitor(tag.expression.as_str(), tag.expression_span),
        TemplateNode::IfBlock(block) => visitor(block.test.as_str(), block.test_span),
        TemplateNode::EachBlock(block) => {
            visitor(block.expression.as_str(), block.expression_span);
            if let Some(span) = block.key_span {
                if let Some(key) = &block.key {
                    visitor(key.as_str(), span);
                }
            }
        }
        TemplateNode::AwaitBlock(block) => {
            visitor(block.expression.as_str(), block.expression_span)
        }
        TemplateNode::KeyBlock(block) => visitor(block.expression.as_str(), block.expression_span),
        TemplateNode::Text(_)
        | TemplateNode::DebugTag(_)
        | TemplateNode::ConstTag(_)
        | TemplateNode::SnippetBlock(_)
        | TemplateNode::Comment(_) => {}
    });
}

#[cfg(test)]
mod tests {
    fn lint(source: &str) -> Vec<crate::linter::LintDiagnostic> {
        let allocator = oxc::allocator::Allocator::default();
        let parsed = crate::parser::parse_for_lint(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        crate::linter::Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/prefer-destructured-store-props")
            .collect()
    }
    #[test]
    fn nested_reactive_labels_do_not_offer_function_local_variables() {
        let source =
            "<script>function helper(){let existing; $: existing=$store.foo;}</script>{$store.foo}";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].suggestions.len(), 1);
        assert!(!diagnostics[0].suggestions[0]
            .description
            .contains("existing"));
    }
    #[test]
    fn suggestions_preserve_nested_members_unicode_and_script_attributes() {
        let source = "<!-- 😀 --><script data-note=\">\">import store from './store'; let foo; $: existing = $store.foo;</script><div data-value={format($store.foo.bar)} {...$store.baz}/>{$store['literal key']}";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 3);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "$store.foo"
        );
        assert_eq!(diagnostics[0].suggestions.len(), 2);
        let fix = &diagnostics[0].suggestions[1].fix;
        let mut output = source.to_string();
        output.replace_range(
            fix.span.start as usize..fix.span.end as usize,
            &fix.replacement,
        );
        assert!(output.contains("$: ({ foo: foo1 } = $store);\n</script>"));
        assert!(output.contains("format(foo1.bar)"));
        assert!(diagnostics[2].suggestions.is_empty());
    }
    #[test]
    fn replacements_avoid_template_and_inline_callback_bindings() {
        let source = "<script>import store from './store'; $: name = $store.name;</script>{#each items as name}{$store.name}{/each}{#await load then name}{$store.name}{/await}{#snippet block(name)}{$store.name}{/snippet}<Comp let:name>{$store.name}</Comp><div>{@const name = 1}{$store.name}</div>{items.map(name => $store.name)}";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 6);
        for diagnostic in diagnostics {
            assert_eq!(diagnostic.suggestions.len(), 1);
            let fix = &diagnostic.suggestions[0].fix;
            let mut output = source.to_string();
            output.replace_range(
                fix.span.start as usize..fix.span.end as usize,
                &fix.replacement,
            );
            assert!(output.contains("$: ({ name: name1 } = $store);"));
        }
    }
    #[test]
    fn template_and_callback_locals_prevent_unsafe_destructuring() {
        let source = "{#each items as key}{$store[key]}{$store.foo}{/each}<Comp let:key>{$store[key]}</Comp>{items.map(key => $store[key])}<svelte:options runes={true}/>{$state.raw}";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "$store.foo"
        );
    }
}
