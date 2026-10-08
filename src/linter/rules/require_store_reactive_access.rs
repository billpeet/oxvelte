//! `svelte/require-store-reactive-access` — require `$store` syntax for reactive access.
//! ⭐ Recommended 🔧 Fixable

use crate::ast::{Attribute, AttributeValue, AttributeValuePart, DirectiveKind, TemplateNode};
use crate::linter::{walk_template_nodes, Fix, LintContext, Rule};
use oxc::ast::ast::{
    BindingPattern, Declaration, Expression, ImportDeclarationSpecifier, ModuleExportName,
    Statement, TSType, TSTypeName, VariableDeclaration, VariableDeclarationKind,
};
use oxc::ast::AstKind;
use oxc::span::{GetSpan, Span};
use std::collections::{HashMap, HashSet};

const STORE_FACTORIES: &[&str] = &["writable", "readable", "derived"];
const RAW_STORE_MSG: &str = "Use the $ prefix or the get function to access reactive values instead of accessing the raw store.";

pub struct RequireStoreReactiveAccess;
impl Rule for RequireStoreReactiveAccess {
    fn name(&self) -> &'static str {
        "svelte/require-store-reactive-access"
    }
    fn is_recommended(&self) -> bool {
        true
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        let Some(script) = &ctx.ast.instance else {
            return;
        };
        let Some(semantic) = ctx.instance_semantic else {
            return;
        };
        let is_typescript =
            script.lang.as_deref() == Some("ts") || script.lang.as_deref() == Some("typescript");
        let imports = collect_imports(semantic.nodes().program().body.as_slice());
        let mut factory_names = HashSet::new();
        for (local, imported, module) in &imports {
            if module == "svelte/store" && STORE_FACTORIES.contains(&imported.as_str()) {
                factory_names.insert(local.clone());
            }
        }
        let mut stores = HashMap::new();
        for statement in &semantic.nodes().program().body {
            match statement {
                Statement::VariableDeclaration(decl) => {
                    collect_store_vars_from_decl(decl, &factory_names, &mut stores)
                }
                Statement::ExportNamedDeclaration(export) => {
                    if let Some(Declaration::VariableDeclaration(decl)) = &export.declaration {
                        collect_store_vars_from_decl(decl, &factory_names, &mut stores);
                    }
                }
                _ => {}
            }
        }
        if is_typescript {
            for node in semantic.nodes().iter() {
                if let AstKind::VariableDeclarator(decl) = node.kind() {
                    if let Some(name) = binding_identifier_name(&decl.id) {
                        if let BindingPattern::BindingIdentifier(binding) = &decl.id {
                            if semantic.scoping().symbol_scope_id(binding.symbol_id())
                                != semantic.scoping().root_scope_id()
                            {
                                continue;
                            }
                        }
                        if let Some(annotation) = &decl.type_annotation {
                            if ts_type_is_store(&annotation.type_annotation) {
                                stores.insert(
                                    name.to_string(),
                                    ts_type_consistently_store(&annotation.type_annotation),
                                );
                            }
                        } else if decl
                            .init
                            .as_ref()
                            .is_some_and(|init| expression_calls_factory(init, &factory_names))
                        {
                            stores.insert(name.to_string(), true);
                        }
                    }
                }
            }
        }
        for (local, imported, module) in &imports {
            if is_typescript && module == "svelte-i18n" && imported != "*" {
                stores.insert(local.clone(), true);
            }
            if !is_typescript || !module.starts_with('.') {
                continue;
            }
            let Some(file_path) = &ctx.file_path else {
                continue;
            };
            let directory = std::path::Path::new(file_path.as_str())
                .parent()
                .unwrap_or(std::path::Path::new("."));
            let Some(content) = resolve_module_file(directory, module) else {
                continue;
            };
            let exports = detect_store_exports(&content);
            for (name, consistent) in exports {
                if imported == "*" {
                    stores.insert(format!("{local}.{name}"), consistent);
                } else if name == *imported {
                    stores.insert(local.clone(), consistent);
                } else if let Some(property) = name.strip_prefix(&format!("{imported}.")) {
                    stores.insert(format!("{local}.{property}"), consistent);
                }
            }
        }
        let scoping = semantic.scoping();
        let mut symbol_stores = HashMap::new();
        for (name, consistent) in &stores {
            if !name.contains('.') {
                if let Some(symbol) =
                    scoping.get_binding(scoping.root_scope_id(), name.as_str().into())
                {
                    symbol_stores.insert(symbol, *consistent);
                }
            }
        }
        let factory_symbols: HashSet<_> = imports
            .iter()
            .filter(|(_, name, module)| {
                module == "svelte/store" && STORE_FACTORIES.contains(&name.as_str())
            })
            .filter_map(|(local, _, _)| {
                scoping.get_binding(scoping.root_scope_id(), local.as_str().into())
            })
            .collect();
        let namespace_symbols: HashSet<_> = imports
            .iter()
            .filter(|(_, name, module)| module == "svelte/store" && name == "*")
            .filter_map(|(local, _, _)| {
                scoping.get_binding(scoping.root_scope_id(), local.as_str().into())
            })
            .collect();
        for node in semantic.nodes().iter() {
            let AstKind::VariableDeclarator(declaration) = node.kind() else {
                continue;
            };
            let BindingPattern::BindingIdentifier(binding) = &declaration.id else {
                continue;
            };
            let factory = match &declaration.init {
                Some(Expression::CallExpression(call)) => match &call.callee {
                    Expression::Identifier(id) => scoping
                        .get_reference(id.reference_id())
                        .symbol_id()
                        .is_some_and(|id| factory_symbols.contains(&id)),
                    Expression::StaticMemberExpression(member)
                        if STORE_FACTORIES.contains(&member.property.name.as_str()) =>
                    {
                        matches!(&member.object, Expression::Identifier(id) if scoping.get_reference(id.reference_id()).symbol_id().is_some_and(|id| namespace_symbols.contains(&id)))
                    }
                    _ => false,
                },
                _ => false,
            };
            if is_typescript {
                if let Some(annotation) = &declaration.type_annotation {
                    if ts_type_is_store(&annotation.type_annotation) {
                        let consistent = ts_type_consistently_store(&annotation.type_annotation);
                        symbol_stores.insert(binding.symbol_id(), consistent);
                        if scoping.symbol_scope_id(binding.symbol_id()) == scoping.root_scope_id() {
                            stores.insert(binding.name.to_string(), consistent);
                        }
                        continue;
                    }
                }
            }
            if factory {
                let consistent = is_typescript
                    || matches!(semantic.nodes().parent_kind(node.id()), AstKind::VariableDeclaration(declaration) if declaration.kind == VariableDeclarationKind::Const);
                symbol_stores.insert(binding.symbol_id(), consistent);
                if scoping.symbol_scope_id(binding.symbol_id()) == scoping.root_scope_id() {
                    stores.insert(binding.name.to_string(), consistent);
                }
            }
        }
        let offset = script.content_span.start;
        for node in semantic.nodes().iter() {
            report_target(
                node.kind(),
                offset,
                &stores,
                Some((semantic, &symbol_stores)),
                ctx,
            );
            for (expression, consistent) in operand_expressions(node.kind()) {
                report_expression(
                    expression,
                    offset,
                    consistent,
                    false,
                    &stores,
                    Some((semantic, &symbol_stores)),
                    ctx,
                );
            }
        }
        // TypeScript narrows nullable stores in the truthy branch of an if block.
        let mut narrowed = Vec::new();
        if is_typescript {
            walk_template_nodes(&ctx.ast.html, &mut |node| {
                if let TemplateNode::IfBlock(block) = node {
                    let name = block.test.trim();
                    if stores.get(name) == Some(&false) {
                        narrowed.push((block.consequent.span, name.to_string()));
                    }
                }
            });
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
        let source = ctx.source;
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            let mut template =
                |text: &str, span: Span, check_root: bool, consistent: bool, disable_fix: bool| {
                    let mut current = stores.clone();
                    for (region, name) in &narrowed {
                        if region.start <= span.start && region.end >= span.end {
                            current.insert(name.clone(), true);
                        }
                    }
                    for (region, names) in &local_regions {
                        if region.start <= span.start && region.end >= span.end {
                            current.retain(|name, _| {
                                !names
                                    .iter()
                                    .any(|local| name.split('.').next() == Some(local.as_str()))
                            });
                        }
                    }
                    check_template(
                        text,
                        span,
                        check_root,
                        consistent,
                        disable_fix,
                        &current,
                        ctx,
                    );
                };
            match node {
                TemplateNode::MustacheTag(tag) => {
                    template(&tag.expression, tag.expression_span, true, false, false)
                }
                TemplateNode::RawMustacheTag(tag) => {
                    template(&tag.expression, tag.expression_span, false, false, false)
                }
                TemplateNode::RenderTag(tag) => {
                    template(&tag.expression, tag.expression_span, false, false, false)
                }
                TemplateNode::IfBlock(block) => {
                    template(&block.test, block.test_span, true, true, false)
                }
                TemplateNode::AwaitBlock(block) => {
                    template(&block.expression, block.expression_span, true, true, false)
                }
                TemplateNode::EachBlock(block) => {
                    template(&block.expression, block.expression_span, true, false, false);
                    if let (Some(key), Some(span)) = (&block.key, block.key_span) {
                        template(key, span, false, false, false);
                    }
                }
                TemplateNode::KeyBlock(block) => template(
                    &block.expression,
                    block.expression_span,
                    false,
                    false,
                    false,
                ),
                TemplateNode::Element(element) => {
                    let accepts_store =
                        !element.kind().is_html() && element.name != "svelte:element";
                    for (index, attribute) in element.attributes.iter().enumerate() {
                        let Some(meta) = element.attribute_meta.get(index) else {
                            continue;
                        };
                        match attribute {
                            Attribute::NormalAttribute { name, value, .. } => {
                                if let AttributeValue::Concat(parts) = value {
                                    for (part_index, part) in parts.iter().enumerate() {
                                        if let (AttributeValuePart::Expression(text), Some(span)) = (
                                            part,
                                            meta.parts
                                                .get(part_index)
                                                .and_then(|p| p.expression_span),
                                        ) {
                                            template(text, span, true, false, false);
                                        }
                                    }
                                } else if let (AttributeValue::Expression(text), Some(span)) =
                                    (value, meta.expression_span)
                                {
                                    let shorthand = meta.equals_span.is_none();
                                    let special_this =
                                        name == "this" && element.kind().is_svelte_special();
                                    template(
                                        text,
                                        span,
                                        special_this || !accepts_store || name.starts_with("--"),
                                        false,
                                        shorthand,
                                    );
                                }
                            }
                            Attribute::Spread { span } => {
                                let expression_span = meta
                                    .expression_span
                                    .unwrap_or(Span::new(span.start + 4, span.end - 1));
                                let text = &source
                                    [expression_span.start as usize..expression_span.end as usize];
                                template(text, expression_span, true, false, false);
                            }
                            Attribute::Directive {
                                kind, name, value, ..
                            } => {
                                let shorthand = meta.equals_span.is_none();
                                if matches!(
                                    kind,
                                    DirectiveKind::Use
                                        | DirectiveKind::Transition
                                        | DirectiveKind::In
                                        | DirectiveKind::Out
                                        | DirectiveKind::Animate
                                ) {
                                    if let Some(span) = meta.directive_subject_span {
                                        template(name, span, true, false, false);
                                    }
                                    if let (AttributeValue::Expression(text), Some(span)) =
                                        (value, meta.expression_span)
                                    {
                                        template(text, span, false, false, false);
                                    }
                                } else if let (AttributeValue::Expression(text), Some(span)) =
                                    (value, meta.expression_span)
                                {
                                    let root = match kind {
                                        DirectiveKind::Binding => !accepts_store || name == "this",
                                        DirectiveKind::Class
                                        | DirectiveKind::EventHandler
                                        | DirectiveKind::StyleDirective => true,
                                        _ => false,
                                    };
                                    template(
                                        text,
                                        span,
                                        root,
                                        matches!(kind, DirectiveKind::Class),
                                        shorthand && !matches!(kind, DirectiveKind::EventHandler),
                                    );
                                } else if shorthand
                                    && matches!(
                                        kind,
                                        DirectiveKind::Binding
                                            | DirectiveKind::Class
                                            | DirectiveKind::StyleDirective
                                    )
                                {
                                    if let Some(span) = meta.directive_subject_span {
                                        template(
                                            name,
                                            span,
                                            !accepts_store
                                                || !matches!(kind, DirectiveKind::Binding)
                                                || name == "this",
                                            matches!(kind, DirectiveKind::Class),
                                            true,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        });
    }
}
fn expression_name(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::Identifier(id) if !id.name.starts_with('$') => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => Some(format!(
            "{}.{}",
            expression_name(&member.object)?,
            member.property.name
        )),
        Expression::ComputedMemberExpression(member) => {
            if let Expression::StringLiteral(property) = &member.expression {
                Some(format!(
                    "{}.{}",
                    expression_name(&member.object)?,
                    property.value
                ))
            } else {
                None
            }
        }
        Expression::ParenthesizedExpression(expression) => expression_name(&expression.expression),
        Expression::TSAsExpression(expression) => expression_name(&expression.expression),
        Expression::TSNonNullExpression(expression) => expression_name(&expression.expression),
        _ => None,
    }
}
fn report_expression(
    expression: &Expression<'_>,
    offset: u32,
    consistent: bool,
    disable_fix: bool,
    stores: &HashMap<String, bool>,
    semantic: Option<(
        &oxc::semantic::Semantic<'_>,
        &HashMap<oxc::semantic::SymbolId, bool>,
    )>,
    ctx: &mut LintContext<'_>,
) {
    let Some(name) = expression_name(expression) else {
        return;
    };
    let expression = expression.get_inner_expression();
    let always_store =
        if let (Some((semantic, symbols)), Expression::Identifier(id)) = (semantic, expression) {
            let Some(symbol) = semantic
                .scoping()
                .get_reference(id.reference_id())
                .symbol_id()
            else {
                return;
            };
            let Some(&consistent) = symbols.get(&symbol) else {
                return;
            };
            consistent
        } else {
            if let (Some((semantic, _)), Some(root_name)) = (semantic, name.split('.').next()) {
                if let Some(reference) = base_identifier(expression) {
                    let scoping = semantic.scoping();
                    if scoping.get_reference(reference.reference_id()).symbol_id()
                        != scoping.get_binding(scoping.root_scope_id(), root_name.into())
                    {
                        return;
                    }
                }
            }
            let Some(&consistent) = stores.get(&name) else {
                return;
            };
            consistent
        };
    if consistent && !always_store {
        return;
    }
    let relative = expression.span();
    let span = Span::new(offset + relative.start, offset + relative.end);
    if matches!(expression, Expression::Identifier(_)) && !disable_fix {
        ctx.diagnostic_with_fix(
            RAW_STORE_MSG,
            span,
            Fix {
                span: Span::new(span.start, span.start),
                replacement: "$".into(),
            },
        );
    } else {
        ctx.diagnostic(RAW_STORE_MSG, span);
    }
}
fn base_identifier<'a>(
    expression: &'a Expression<'a>,
) -> Option<&'a oxc::ast::ast::IdentifierReference<'a>> {
    match expression {
        Expression::Identifier(id) => Some(id),
        Expression::StaticMemberExpression(member) => base_identifier(&member.object),
        Expression::ComputedMemberExpression(member) => base_identifier(&member.object),
        _ => None,
    }
}
fn check_template(
    text: &str,
    expression_span: Span,
    root: bool,
    consistent: bool,
    disable_fix: bool,
    stores: &HashMap<String, bool>,
    ctx: &mut LintContext<'_>,
) {
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
    let offset = expression_span.start - 1;
    if root {
        if let Statement::ExpressionStatement(statement) = &parsed.program.body[0] {
            let expression = statement.expression.get_inner_expression();
            if expression_is_local(expression, &semantic) {
                return;
            }
            report_expression(
                expression,
                offset,
                consistent,
                disable_fix,
                stores,
                None,
                ctx,
            );
        }
    }
    for node in semantic.nodes().iter() {
        if !target_is_local(node.kind(), &semantic) {
            report_target(node.kind(), offset, stores, None, ctx);
        }
        for (expression, consistent) in operand_expressions(node.kind()) {
            if !expression_is_local(expression, &semantic) {
                report_expression(expression, offset, consistent, false, stores, None, ctx);
            }
        }
    }
}
fn expression_is_local(
    expression: &Expression<'_>,
    semantic: &oxc::semantic::Semantic<'_>,
) -> bool {
    base_identifier(expression.get_inner_expression()).is_some_and(|id| {
        semantic
            .scoping()
            .get_reference(id.reference_id())
            .symbol_id()
            .is_some()
    })
}
fn target_is_local(kind: AstKind<'_>, semantic: &oxc::semantic::Semantic<'_>) -> bool {
    use oxc::ast::ast::SimpleAssignmentTarget;
    let target = match kind {
        AstKind::UpdateExpression(node) => Some(&node.argument),
        AstKind::AssignmentExpression(node) => node.left.as_simple_assignment_target(),
        _ => None,
    };
    match target {
        Some(SimpleAssignmentTarget::AssignmentTargetIdentifier(id)) => semantic
            .scoping()
            .get_reference(id.reference_id())
            .symbol_id()
            .is_some(),
        Some(SimpleAssignmentTarget::StaticMemberExpression(member)) => {
            expression_is_local(&member.object, semantic)
        }
        _ => false,
    }
}
fn report_target(
    kind: AstKind<'_>,
    offset: u32,
    stores: &HashMap<String, bool>,
    semantic: Option<(
        &oxc::semantic::Semantic<'_>,
        &HashMap<oxc::semantic::SymbolId, bool>,
    )>,
    ctx: &mut LintContext<'_>,
) {
    use oxc::ast::ast::SimpleAssignmentTarget;
    let target = match kind {
        AstKind::UpdateExpression(update) => Some(&update.argument),
        AstKind::AssignmentExpression(assignment)
            if assignment.operator != oxc::syntax::operator::AssignmentOperator::Assign =>
        {
            assignment.left.as_simple_assignment_target()
        }
        _ => None,
    };
    let Some(target) = target else { return };
    match target {
        SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
            let store = if let Some((semantic, symbols)) = semantic {
                semantic
                    .scoping()
                    .get_reference(id.reference_id())
                    .symbol_id()
                    .is_some_and(|id| symbols.contains_key(&id))
            } else {
                stores.contains_key(id.name.as_str())
            };
            if store {
                let span = Span::new(offset + id.span.start, offset + id.span.end);
                ctx.diagnostic_with_fix(
                    RAW_STORE_MSG,
                    span,
                    Fix {
                        span: Span::new(span.start, span.start),
                        replacement: "$".into(),
                    },
                );
            }
        }
        SimpleAssignmentTarget::StaticMemberExpression(member) => {
            if let Some((semantic, _)) = semantic {
                if let Some(reference) = base_identifier(&member.object) {
                    let scoping = semantic.scoping();
                    if scoping.get_reference(reference.reference_id()).symbol_id()
                        != scoping
                            .get_binding(scoping.root_scope_id(), reference.name.as_str().into())
                    {
                        return;
                    }
                }
            }
            if let Some(object) = expression_name(&member.object) {
                if stores.contains_key(&format!("{object}.{}", member.property.name)) {
                    ctx.diagnostic(
                        RAW_STORE_MSG,
                        Span::new(offset + member.span.start, offset + member.span.end),
                    );
                }
            }
        }
        _ => {}
    }
}
fn operand_expressions<'a>(kind: AstKind<'a>) -> Vec<(&'a Expression<'a>, bool)> {
    match kind {
        AstKind::IfStatement(node) => vec![(&node.test, true)],
        AstKind::WhileStatement(node) => vec![(&node.test, true)],
        AstKind::DoWhileStatement(node) => vec![(&node.test, true)],
        AstKind::ConditionalExpression(node) => vec![(&node.test, true)],
        AstKind::ForStatement(node) => node
            .test
            .as_ref()
            .map(|test| vec![(test, true)])
            .unwrap_or_default(),
        AstKind::ForInStatement(node) => vec![(&node.right, false)],
        AstKind::ForOfStatement(node) => vec![(&node.right, false)],
        AstKind::SwitchStatement(node) => vec![(&node.discriminant, false)],
        AstKind::CallExpression(node) => vec![(&node.callee, false)],
        AstKind::NewExpression(node) => vec![(&node.callee, false)],
        AstKind::UnaryExpression(node) => vec![(
            &node.argument,
            matches!(
                node.operator,
                oxc::syntax::operator::UnaryOperator::LogicalNot
                    | oxc::syntax::operator::UnaryOperator::Typeof
            ),
        )],
        AstKind::SpreadElement(node) => vec![(&node.argument, false)],
        AstKind::AssignmentExpression(node)
            if node.operator != oxc::syntax::operator::AssignmentOperator::Assign =>
        {
            vec![(&node.right, false)]
        }
        AstKind::BinaryExpression(node) => {
            let consistent = node.operator.is_equality();
            vec![(&node.left, consistent), (&node.right, consistent)]
        }
        AstKind::LogicalExpression(node) => vec![(&node.left, true)],
        AstKind::TemplateLiteral(node) => node
            .expressions
            .iter()
            .map(|expression| (expression, false))
            .collect(),
        AstKind::TaggedTemplateExpression(node) => vec![(&node.tag, false)],
        AstKind::ObjectProperty(node) if node.computed => node
            .key
            .as_expression()
            .map(|expression| vec![(expression, false)])
            .unwrap_or_default(),
        AstKind::PropertyDefinition(node) if node.computed => node
            .key
            .as_expression()
            .map(|expression| vec![(expression, false)])
            .unwrap_or_default(),
        AstKind::MethodDefinition(node) if node.computed => node
            .key
            .as_expression()
            .map(|expression| vec![(expression, false)])
            .unwrap_or_default(),
        AstKind::ImportExpression(node) => vec![(&node.source, false)],
        AstKind::AwaitExpression(node) => vec![(&node.argument, true)],
        _ => Vec::new(),
    }
}
fn ts_type_consistently_store(ty: &TSType<'_>) -> bool {
    match ty {
        TSType::TSUnionType(union) => union.types.iter().all(ts_type_consistently_store),
        TSType::TSParenthesizedType(ty) => ts_type_consistently_store(&ty.type_annotation),
        _ => ts_type_is_store(ty),
    }
}
/// Record each identifier binding from a VariableDeclaration as a store when
/// the declarator's initializer is `factory(...)` or the declarator has a
/// store-typed annotation. The declaration's kind determines whether the
/// resulting map entry is `true` (const) or `false` (let/var).
fn collect_store_vars_from_decl(
    vd: &VariableDeclaration<'_>,
    factory_names: &HashSet<String>,
    out: &mut HashMap<String, bool>,
) {
    let is_const = matches!(
        vd.kind,
        VariableDeclarationKind::Const
            | VariableDeclarationKind::Using
            | VariableDeclarationKind::AwaitUsing
    );
    for d in &vd.declarations {
        let Some(name) = binding_identifier_name(&d.id) else {
            continue;
        };
        let init_is_factory = d
            .init
            .as_ref()
            .map(|e| expression_calls_factory(e, factory_names))
            .unwrap_or(false);
        let typed_as_store = d
            .type_annotation
            .as_ref()
            .map(|ta| ts_type_is_store(&ta.type_annotation))
            .unwrap_or(false);
        if init_is_factory || typed_as_store {
            out.insert(name.to_string(), is_const);
        }
    }
}

fn binding_identifier_name<'a>(pat: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pat {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// True iff the expression is a direct `name(...)` call whose callee matches
/// one of the known store-factory names.
fn expression_calls_factory(expr: &Expression<'_>, factory_names: &HashSet<String>) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    factory_names.contains(id.name.as_str())
}

/// True iff the TS type names a store (directly, or as a member of a union).
fn ts_type_is_store(ty: &TSType<'_>) -> bool {
    match ty {
        TSType::TSTypeReference(r) => {
            let TSTypeName::IdentifierReference(id) = &r.type_name else {
                return false;
            };
            matches!(id.name.as_str(), "Writable" | "Readable" | "Derived")
        }
        TSType::TSUnionType(u) => u.types.iter().any(ts_type_is_store),
        TSType::TSParenthesizedType(p) => ts_type_is_store(&p.type_annotation),
        _ => false,
    }
}

/// Collect `(local, imported, module)` triples from top-level `ImportDeclaration`
/// statements. `imported` is `"default"` for default imports and `"*"` for
/// namespace imports, matching the legacy `parse_imports` shape.
fn collect_imports(body: &[Statement<'_>]) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for stmt in body {
        let Statement::ImportDeclaration(imp) = stmt else {
            continue;
        };
        let source = imp.source.value.as_str().to_string();
        let Some(specs) = &imp.specifiers else {
            continue;
        };
        for spec in specs {
            match spec {
                ImportDeclarationSpecifier::ImportSpecifier(s) => {
                    let imported = match &s.imported {
                        ModuleExportName::IdentifierName(n) => n.name.as_str().to_string(),
                        ModuleExportName::IdentifierReference(n) => n.name.as_str().to_string(),
                        ModuleExportName::StringLiteral(l) => l.value.as_str().to_string(),
                    };
                    out.push((s.local.name.as_str().to_string(), imported, source.clone()));
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                    out.push((
                        s.local.name.as_str().to_string(),
                        "default".to_string(),
                        source.clone(),
                    ));
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                    out.push((
                        s.local.name.as_str().to_string(),
                        "*".to_string(),
                        source.clone(),
                    ));
                }
            }
        }
    }
    out
}

fn resolve_module_file(dir: &std::path::Path, module: &str) -> Option<String> {
    ["", ".ts", ".js", ".d.ts"]
        .iter()
        .find_map(|ext| std::fs::read_to_string(dir.join(format!("{}{}", module, ext))).ok())
}

fn detect_store_exports(content: &str) -> HashMap<String, bool> {
    use oxc::allocator::Allocator;
    use oxc::parser::Parser;
    use oxc::span::SourceType;

    let alloc = Allocator::default();
    let parsed = Parser::new(&alloc, content, SourceType::ts()).parse();
    let body = parsed.program.body.as_slice();

    // Collect factory-name and store-type aliases from `svelte/store` imports.
    let (mut factory_names, mut store_type_names): (HashSet<String>, HashSet<String>) =
        (HashSet::new(), HashSet::new());
    for stmt in body {
        let Statement::ImportDeclaration(imp) = stmt else {
            continue;
        };
        if imp.source.value != "svelte/store" {
            continue;
        }
        let Some(specs) = &imp.specifiers else {
            continue;
        };
        for spec in specs {
            let ImportDeclarationSpecifier::ImportSpecifier(s) = spec else {
                continue;
            };
            let imported = match &s.imported {
                ModuleExportName::IdentifierName(n) => n.name.as_str(),
                ModuleExportName::IdentifierReference(n) => n.name.as_str(),
                ModuleExportName::StringLiteral(l) => l.value.as_str(),
            };
            let local = s.local.name.as_str().to_string();
            if STORE_FACTORIES.contains(&imported) {
                factory_names.insert(local.clone());
            }
            if matches!(imported, "Writable" | "Readable" | "Derived") {
                store_type_names.insert(local);
            }
        }
    }

    // Index interfaces that extend a store type, so type annotations like
    // `MyStore` (where `interface MyStore extends Writable<T>`) are recognised.
    let mut store_interfaces: HashSet<String> = HashSet::new();
    for stmt in body {
        if let Statement::TSInterfaceDeclaration(iface) = stmt {
            let extends_store = iface.extends.iter().any(|h| {
                let Expression::Identifier(id) = &h.expression else {
                    return false;
                };
                matches!(id.name.as_str(), "Writable" | "Readable" | "Derived")
                    || store_type_names.contains(id.name.as_str())
            });
            if extends_store {
                store_interfaces.insert(iface.id.name.as_str().to_string());
            }
        }
    }

    // First pass: exported VariableDeclarations whose init is a factory call
    // or whose binding has a store-typed annotation. Record (name, is_const)
    // using the enclosing VariableDeclaration's `kind`.
    let mut stores: HashMap<String, bool> = HashMap::new();
    // Each entry is (declarator, is_const) so pass 2 can propagate is_const.
    let mut exported_decls: Vec<(&oxc::ast::ast::VariableDeclarator, bool)> = Vec::new();
    for stmt in body {
        let Statement::ExportNamedDeclaration(exp) = stmt else {
            continue;
        };
        let Some(Declaration::VariableDeclaration(vd)) = &exp.declaration else {
            continue;
        };
        let is_const = matches!(
            vd.kind,
            VariableDeclarationKind::Const
                | VariableDeclarationKind::Using
                | VariableDeclarationKind::AwaitUsing
        );
        for d in &vd.declarations {
            exported_decls.push((d, is_const));
            let Some(name) = binding_identifier_name(&d.id) else {
                continue;
            };
            let init_store = d
                .init
                .as_ref()
                .map(|e| {
                    if expression_calls_factory(e, &factory_names) {
                        return true;
                    }
                    false
                })
                .unwrap_or(false);
            let typed_store = d
                .type_annotation
                .as_ref()
                .map(|ta| {
                    ts_type_is_store(&ta.type_annotation)
                        || ts_type_references_names(&ta.type_annotation, &store_type_names)
                        || ts_type_references_names(&ta.type_annotation, &store_interfaces)
                })
                .unwrap_or(false);
            if init_store || typed_store {
                let consistent = d.type_annotation.as_ref().map_or(true, |annotation| {
                    ts_type_consistently_store(&annotation.type_annotation)
                        || ts_type_consistently_references_names(
                            &annotation.type_annotation,
                            &store_type_names,
                        )
                        || ts_type_consistently_references_names(
                            &annotation.type_annotation,
                            &store_interfaces,
                        )
                });
                stores.insert(name.to_string(), consistent);
            }
        }
    }

    // Infer store-valued object properties without treating their containers as stores.
    loop {
        let mut additions = HashMap::new();
        for (declaration, _) in &exported_decls {
            let Some(name) = binding_identifier_name(&declaration.id) else {
                continue;
            };
            let Some(initializer) = &declaration.init else {
                continue;
            };
            if let Expression::ObjectExpression(object) = initializer {
                for property in &object.properties {
                    let oxc::ast::ast::ObjectPropertyKind::ObjectProperty(property) = property
                    else {
                        continue;
                    };
                    let key = match &property.key {
                        oxc::ast::ast::PropertyKey::StaticIdentifier(key) => {
                            Some(key.name.to_string())
                        }
                        oxc::ast::ast::PropertyKey::StringLiteral(key) => {
                            Some(key.value.to_string())
                        }
                        _ => None,
                    };
                    if let (Some(key), Some(value)) = (key, expression_name(&property.value)) {
                        if let Some(&consistent) = stores.get(&value) {
                            additions.insert(format!("{name}.{key}"), consistent);
                        }
                    }
                }
            } else if let Some(value) = expression_name(initializer) {
                if let Some(&consistent) = stores.get(&value) {
                    additions.insert(name.to_string(), consistent);
                }
            }
        }
        let previous = stores.len();
        stores.extend(additions);
        if previous == stores.len() {
            break;
        }
    }

    stores
}

/// True iff `ty` is a `TSTypeReference` (possibly nested in a union/paren)
/// whose outer identifier is in `names`.
fn ts_type_references_names(ty: &TSType<'_>, names: &HashSet<String>) -> bool {
    match ty {
        TSType::TSTypeReference(r) => {
            let TSTypeName::IdentifierReference(id) = &r.type_name else {
                return false;
            };
            names.contains(id.name.as_str())
        }
        TSType::TSUnionType(u) => u.types.iter().any(|t| ts_type_references_names(t, names)),
        TSType::TSParenthesizedType(p) => ts_type_references_names(&p.type_annotation, names),
        _ => false,
    }
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

fn ts_type_consistently_references_names(ty: &TSType<'_>, names: &HashSet<String>) -> bool {
    match ty {
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .all(|ty| ts_type_consistently_references_names(ty, names)),
        TSType::TSParenthesizedType(ty) => {
            ts_type_consistently_references_names(&ty.type_annotation, names)
        }
        _ => ts_type_references_names(ty, names),
    }
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
            .filter(|d| d.rule_name == "svelte/require-store-reactive-access")
            .collect()
    }
    #[test]
    fn ast_operands_avoid_text_and_keep_precise_unicode_fix_locations() {
        let source = "<!-- 😀 --><script data-note=\">\">import {writable as make} from 'svelte/store'; const store = make(1); const text = 'store + 1'; /* store + 1 */ store + 1; const kept = store; pass(store); store += 1; store++; function shadow(store) { store + 1; } if ((store)) {}</script><p>{store}</p><input {store}/>";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 6);
        assert_eq!(diagnostics.iter().filter(|d| d.fix.is_some()).count(), 5);
        for diagnostic in &diagnostics {
            assert_eq!(
                &source[diagnostic.span.start as usize..diagnostic.span.end as usize],
                "store"
            );
        }
        for diagnostic in diagnostics.iter().filter(|d| d.fix.is_some()) {
            let fix = diagnostic.fix.as_ref().unwrap();
            assert_eq!(fix.span.start, diagnostic.span.start);
            assert_eq!(fix.span.start, fix.span.end);
            assert_eq!(fix.replacement, "$");
        }
    }
    #[test]
    fn nested_store_factories_and_types_use_binding_identity() {
        let source = "<script lang=\"ts\">import * as stores from 'svelte/store'; import {Writable, writable as make} from 'svelte/store'; let value = 1; function typed(){ let value: Writable<number>; value + 1; } function imported(){ const nested = stores.writable(1); if (nested){} } function shadow(make){ const fake = make(1); fake + 1; } value + 1;</script>{value}";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            "value"
        );
        assert_eq!(
            &source[diagnostics[1].span.start as usize..diagnostics[1].span.end as usize],
            "nested"
        );
    }
    #[test]
    fn template_locals_and_separate_callback_scopes_do_not_hide_outer_stores() {
        let source = "<script>import {writable} from 'svelte/store'; const store = writable(1);</script>{#each items as store}{store}{/each}{#await load then store}{store}{/await}{#snippet block(store)}{store}{/snippet}<Comp let:store>{store}</Comp><div>{@const store = 1}{store}</div>{store + (() => { let store = 1; return store + 1; })()}";
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].span.start as usize,
            source.rfind("{store +").unwrap() + 1
        );
    }
    #[test]
    fn imported_object_properties_do_not_make_containers_or_functions_stores() {
        let exports = super::detect_store_exports("import {writable} from 'svelte/store'; const local = writable(1); export const store = writable(1); export const object = {value: store}; export const factory = () => store; export const ordinary = consume(store);");
        assert!(exports.contains_key("store"));
        assert!(exports.contains_key("object.value"));
        assert!(!exports.contains_key("object"));
        assert!(!exports.contains_key("factory"));
        assert!(!exports.contains_key("ordinary"));
    }
}
