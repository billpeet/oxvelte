//! `@typescript-eslint/no-unused-vars` for `.svelte` components.
//!
//! Script-only linters cannot run this rule on a component, because a name declared in
//! `<script>` may be used only by the template. This rule reads the script's semantic model
//! and counts template usage as well.
//!
//! It is deliberately conservative: when it cannot prove a name is unused, it stays quiet.
//! Template usage is matched by name without regard to shadowing, a function that only calls
//! itself counts as used, and `x += 1` counts as a read.

use crate::ast::{Attribute, AttributeValue, DirectiveKind, TemplateNode};
use crate::linter::{walk_template_nodes, LintContext, Rule};
use oxc::ast::AstKind;
use oxc::semantic::{Semantic, SymbolFlags, SymbolId};
use oxc::span::{GetSpan, Span};
use regex::Regex;
use rustc_hash::FxHashSet;

pub struct NoUnusedVars;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArgsMode {
    AfterUsed,
    All,
    None,
}

struct Options {
    vars_local: bool,
    args: ArgsMode,
    caught_errors: bool,
    ignore_rest_siblings: bool,
    vars_ignore: Option<Regex>,
    args_ignore: Option<Regex>,
    caught_errors_ignore: Option<Regex>,
    destructured_array_ignore: Option<Regex>,
}

impl Options {
    fn from_config(options: &Option<serde_json::Value>) -> Self {
        let first = options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|a| a.first());
        let mut parsed = Self {
            vars_local: false,
            args: ArgsMode::AfterUsed,
            caught_errors: true,
            ignore_rest_siblings: false,
            vars_ignore: None,
            args_ignore: None,
            caught_errors_ignore: None,
            destructured_array_ignore: None,
        };
        let Some(first) = first else { return parsed };
        if let Some(vars) = first.as_str() {
            parsed.vars_local = vars == "local";
            return parsed;
        }
        let text = |key: &str| first.get(key).and_then(|v| v.as_str());
        let pattern = |key: &str| text(key).and_then(|p| Regex::new(p).ok());
        parsed.vars_local = text("vars") == Some("local");
        parsed.args = match text("args") {
            Some("all") => ArgsMode::All,
            Some("none") => ArgsMode::None,
            _ => ArgsMode::AfterUsed,
        };
        parsed.caught_errors = text("caughtErrors") != Some("none");
        parsed.ignore_rest_siblings = first
            .get("ignoreRestSiblings")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        parsed.vars_ignore = pattern("varsIgnorePattern");
        parsed.args_ignore = pattern("argsIgnorePattern");
        parsed.caught_errors_ignore = pattern("caughtErrorsIgnorePattern");
        parsed.destructured_array_ignore = pattern("destructuredArrayIgnorePattern");
        parsed
    }
}

/// What kind of declaration a symbol comes from, as far as this rule cares.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DeclKind {
    Variable,
    Parameter,
    CaughtError,
}

struct Declaration {
    kind: DeclKind,
    /// The binding sits inside an array destructuring pattern.
    in_array_pattern: bool,
    /// The declaration itself assigns a value: an initialiser, a default or a loop binding.
    has_initial_value: bool,
}

impl Rule for NoUnusedVars {
    fn name(&self) -> &'static str {
        "@typescript-eslint/no-unused-vars"
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if ctx.is_svelte_module {
            return;
        }
        let options = Options::from_config(&ctx.config.options);

        // Names the template and the `<script>` open tags refer to.
        let mut outside_names = template_names(ctx);
        // Names one script leaves unresolved, which the other script or a `$store`
        // subscription may supply.
        for semantic in [ctx.instance_semantic, ctx.module_semantic]
            .into_iter()
            .flatten()
        {
            for name in semantic.scoping().root_unresolved_references().keys() {
                insert_name(&mut outside_names, name.as_str());
            }
        }

        let mut findings: Vec<(Span, String)> = Vec::new();
        for (semantic, offset) in [
            (ctx.module_semantic, ctx.module_content_offset),
            (ctx.instance_semantic, ctx.instance_content_offset),
        ] {
            let Some(semantic) = semantic else { continue };
            check_script(semantic, offset, &options, &outside_names, &mut findings);
        }
        findings.sort_by_key(|(span, _)| span.start);
        for (span, message) in findings {
            ctx.diagnostic(message, span);
        }
    }
}

fn check_script(
    semantic: &Semantic<'_>,
    offset: u32,
    options: &Options,
    outside_names: &Names,
    findings: &mut Vec<(Span, String)>,
) {
    let scoping = semantic.scoping();
    let root_scope = scoping.root_scope_id();

    for symbol in scoping.symbol_ids() {
        let flags = scoping.symbol_flags(symbol);
        // Type parameters, enum members and namespaces need rules of their own.
        if flags.intersects(
            SymbolFlags::TypeParameter
                | SymbolFlags::EnumMember
                | SymbolFlags::NamespaceModule
                | SymbolFlags::ValueModule
                | SymbolFlags::Ambient,
        ) {
            continue;
        }
        let name = scoping.symbol_name(symbol);
        let is_root = scoping.symbol_scope_id(symbol) == root_scope;
        if is_root && (options.vars_local || outside_names.read.contains(name)) {
            continue;
        }
        if is_used_in_script(semantic, symbol) {
            continue;
        }
        let Some(declaration) = classify(semantic, symbol, options) else {
            continue;
        };

        let pattern = match declaration.kind {
            DeclKind::CaughtError => {
                if !options.caught_errors {
                    continue;
                }
                &options.caught_errors_ignore
            }
            DeclKind::Parameter => {
                if options.args == ArgsMode::None {
                    continue;
                }
                &options.args_ignore
            }
            DeclKind::Variable => &options.vars_ignore,
        };
        if pattern.as_ref().is_some_and(|p| p.is_match(name)) {
            continue;
        }
        if declaration.in_array_pattern
            && options
                .destructured_array_ignore
                .as_ref()
                .is_some_and(|p| p.is_match(name))
        {
            continue;
        }

        // typescript-eslint reports at the last assignment in the declaring function, or at
        // the declaration when there is none.
        let last_write = last_write_span(semantic, symbol);
        let assigned = declaration.has_initial_value
            || last_write.is_some()
            || (is_root && outside_names.assigned.contains(name));
        let span = last_write.unwrap_or_else(|| scoping.symbol_span(symbol));

        let (described, described_pattern) = match declaration.kind {
            DeclKind::CaughtError => ("caught errors", &options.caught_errors_ignore),
            DeclKind::Parameter => ("args", &options.args_ignore),
            DeclKind::Variable => {
                if declaration.in_array_pattern && options.destructured_array_ignore.is_some() {
                    (
                        "elements of array destructuring",
                        &options.destructured_array_ignore,
                    )
                } else {
                    ("vars", &options.vars_ignore)
                }
            }
        };
        let action = if assigned {
            "assigned a value"
        } else {
            "defined"
        };
        let additional = described_pattern
            .as_ref()
            .map(|p| format!(". Allowed unused {described} must match /{}/u", p.as_str()))
            .unwrap_or_default();
        findings.push((
            Span::new(offset + span.start, offset + span.end),
            format!("'{name}' is {action} but never used{additional}."),
        ));
    }
}

/// A symbol is used when the script reads it or names it as a type. A reference that only
/// writes does not count, and neither does one that reads the value only to update it,
/// such as `total += 1;` or `count++;` as a statement.
fn is_used_in_script(semantic: &Semantic<'_>, symbol: SymbolId) -> bool {
    semantic
        .scoping()
        .get_resolved_references(symbol)
        .any(|reference| {
            if !reference.is_write() {
                return true;
            }
            reference.is_read() && !updates_itself_only(semantic, reference.node_id())
        })
}

/// `x += 1;` and `x++;` as whole statements read `x` only to write it back.
fn updates_itself_only(semantic: &Semantic<'_>, reference_node: oxc::semantic::NodeId) -> bool {
    use oxc::ast::ast::AssignmentOperator;
    let mut ancestors = semantic.nodes().ancestor_kinds(reference_node);
    let Some(parent) = ancestors.next() else {
        return false;
    };
    let is_update = match parent {
        AstKind::UpdateExpression(_) => true,
        AstKind::AssignmentExpression(assignment) => !matches!(
            assignment.operator,
            AssignmentOperator::Assign
                | AssignmentOperator::LogicalAnd
                | AssignmentOperator::LogicalOr
                | AssignmentOperator::LogicalNullish
        ),
        _ => false,
    };
    is_update && matches!(ancestors.next(), Some(AstKind::ExpressionStatement(_)))
}

/// The span of the last write-only reference in the symbol's own function, if any.
fn last_write_span(semantic: &Semantic<'_>, symbol: SymbolId) -> Option<Span> {
    let scoping = semantic.scoping();
    let home = variable_scope(semantic, scoping.symbol_scope_id(symbol));
    scoping
        .get_resolved_references(symbol)
        .filter(|reference| reference.is_write())
        .filter(|reference| variable_scope(semantic, reference.scope_id()) == home)
        .map(|reference| semantic.nodes().kind(reference.node_id()).span())
        .max_by_key(|span| span.start)
}

/// The nearest enclosing function or top-level scope.
fn variable_scope(
    semantic: &Semantic<'_>,
    scope: oxc::semantic::ScopeId,
) -> oxc::semantic::ScopeId {
    let scoping = semantic.scoping();
    let mut current = scope;
    loop {
        if scoping.scope_flags(current).is_var() {
            return current;
        }
        match scoping.scope_parent_id(current) {
            Some(parent) => current = parent,
            None => return current,
        }
    }
}

/// Work out what declared the symbol. Returns `None` for declarations this rule leaves alone:
/// exports, ambient declarations, signatures without a body, and parameters that the `args`
/// option excuses.
fn classify(semantic: &Semantic<'_>, symbol: SymbolId, options: &Options) -> Option<Declaration> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let declaration_node = scoping.symbol_declaration(symbol);
    let flags = scoping.symbol_flags(symbol);

    let mut declaration = Declaration {
        kind: if flags.contains(SymbolFlags::CatchVariable) {
            DeclKind::CaughtError
        } else {
            DeclKind::Variable
        },
        in_array_pattern: false,
        has_initial_value: false,
    };

    let target = scoping.symbol_span(symbol);
    let mut seen_own_declaration = false;
    let mut parameter: Option<Span> = None;
    for kind in
        std::iter::once(nodes.kind(declaration_node)).chain(nodes.ancestor_kinds(declaration_node))
    {
        match kind {
            AstKind::VariableDeclarator(declarator) => {
                if declarator.init.is_some() {
                    declaration.has_initial_value = true;
                }
                if !apply_pattern(&declarator.id, target, options, &mut declaration) {
                    return None;
                }
            }
            AstKind::VariableDeclaration(variable) => {
                if variable.declare {
                    return None;
                }
            }
            AstKind::ForInStatement(_) | AstKind::ForOfStatement(_) => {
                if !seen_own_declaration {
                    declaration.has_initial_value = true;
                }
            }
            AstKind::CatchParameter(param) => {
                declaration.kind = DeclKind::CaughtError;
                if !apply_pattern(&param.pattern, target, options, &mut declaration) {
                    return None;
                }
            }
            AstKind::FormalParameter(param) => {
                declaration.kind = DeclKind::Parameter;
                if param.initializer.is_some() {
                    declaration.has_initial_value = true;
                }
                // `constructor(private x: T)` declares a class property.
                if param.accessibility.is_some() || param.readonly || param.r#override {
                    return None;
                }
                if !apply_pattern(&param.pattern, target, options, &mut declaration) {
                    return None;
                }
                parameter = Some(param.span);
            }
            AstKind::FormalParameters(params) => {
                if let Some(own) = parameter {
                    if options.args == ArgsMode::AfterUsed
                        && later_parameter_is_used(semantic, params, own)
                    {
                        return None;
                    }
                }
            }
            AstKind::Function(function) => {
                if function.declare || function.body.is_none() {
                    return None;
                }
                if seen_own_declaration || parameter.is_some() {
                    break;
                }
            }
            AstKind::ArrowFunctionExpression(_) => {
                if seen_own_declaration || parameter.is_some() {
                    break;
                }
            }
            AstKind::Class(class) => {
                if class.declare {
                    return None;
                }
                if seen_own_declaration {
                    break;
                }
            }
            AstKind::TSEnumDeclaration(declared) => {
                if declared.declare {
                    return None;
                }
            }
            AstKind::TSInterfaceDeclaration(declared) => {
                if declared.declare {
                    return None;
                }
            }
            AstKind::TSTypeAliasDeclaration(declared) => {
                if declared.declare {
                    return None;
                }
            }
            // A parameter of a type or a signature, not of code that runs.
            AstKind::TSFunctionType(_)
            | AstKind::TSConstructorType(_)
            | AstKind::TSMethodSignature(_)
            | AstKind::TSCallSignatureDeclaration(_)
            | AstKind::TSConstructSignatureDeclaration(_)
            | AstKind::TSIndexSignature(_)
            | AstKind::TSModuleDeclaration(_)
            | AstKind::TSGlobalDeclaration(_) => return None,
            AstKind::ExportNamedDeclaration(_)
            | AstKind::ExportDefaultDeclaration(_)
            | AstKind::ExportAllDeclaration(_) => return None,
            AstKind::StaticBlock(_) | AstKind::Program(_) => break,
            _ => {}
        }
        seen_own_declaration = true;
    }
    Some(declaration)
}

/// Where a binding sits inside a destructuring pattern.
#[derive(Default)]
struct PatternPlace {
    /// Directly an element of an array pattern.
    array_element: bool,
    /// Has a default value, `{ a = 1 }`.
    has_default: bool,
    /// A property of an object pattern that also has a rest element.
    rest_sibling: bool,
    /// Defaults to `$bindable(...)`: the component writes it for its parent to read.
    bindable: bool,
}

/// Record where `target` sits in `pattern`. Returns false when the binding is excused: a
/// `$bindable()` prop, or a rest sibling under `ignoreRestSiblings`.
fn apply_pattern(
    pattern: &oxc::ast::ast::BindingPattern<'_>,
    target: Span,
    options: &Options,
    declaration: &mut Declaration,
) -> bool {
    let mut place = PatternPlace::default();
    if !locate(pattern, target, false, &mut place) {
        return true;
    }
    declaration.in_array_pattern = place.array_element;
    if place.has_default {
        declaration.has_initial_value = true;
    }
    !(place.bindable || options.ignore_rest_siblings && place.rest_sibling)
}

fn locate(
    pattern: &oxc::ast::ast::BindingPattern<'_>,
    target: Span,
    in_array: bool,
    place: &mut PatternPlace,
) -> bool {
    use oxc::ast::ast::BindingPattern;
    match pattern {
        BindingPattern::BindingIdentifier(ident) => {
            if ident.span != target {
                return false;
            }
            place.array_element = in_array;
            true
        }
        BindingPattern::AssignmentPattern(assignment) => {
            if !locate(&assignment.left, target, false, place) {
                return false;
            }
            if is_identifier_at(&assignment.left, target) {
                place.has_default = true;
                place.bindable = matches!(
                    &assignment.right,
                    oxc::ast::ast::Expression::CallExpression(call)
                        if matches!(&call.callee, oxc::ast::ast::Expression::Identifier(callee) if callee.name == "$bindable")
                );
            }
            true
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                if locate(&property.value, target, false, place) {
                    let direct = is_identifier_at(&property.value, target)
                        || matches!(&property.value, BindingPattern::AssignmentPattern(a) if is_identifier_at(&a.left, target));
                    if direct && object.rest.is_some() {
                        place.rest_sibling = true;
                    }
                    return true;
                }
            }
            object
                .rest
                .as_ref()
                .is_some_and(|rest| locate(&rest.argument, target, false, place))
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                if locate(element, target, true, place) {
                    return true;
                }
            }
            array
                .rest
                .as_ref()
                .is_some_and(|rest| locate(&rest.argument, target, false, place))
        }
    }
}

fn is_identifier_at(pattern: &oxc::ast::ast::BindingPattern<'_>, target: Span) -> bool {
    matches!(pattern, oxc::ast::ast::BindingPattern::BindingIdentifier(ident) if ident.span == target)
}

/// `args: "after-used"`: a parameter is excused when a later parameter of the same function
/// is used.
fn later_parameter_is_used(
    semantic: &Semantic<'_>,
    params: &oxc::ast::ast::FormalParameters<'_>,
    own: Span,
) -> bool {
    let is_used = |symbol: Option<SymbolId>| symbol.is_some_and(|s| is_used_in_script(semantic, s));
    let later_item = params
        .items
        .iter()
        .filter(|item| item.span.start > own.start)
        .flat_map(|item| item.pattern.get_binding_identifiers())
        .any(|ident| is_used(ident.symbol_id.get()));
    let rest = params.rest.as_ref().is_some_and(|rest| {
        rest.span.start > own.start
            && rest
                .rest
                .argument
                .get_binding_identifiers()
                .iter()
                .any(|ident| is_used(ident.symbol_id.get()))
    });
    later_item || rest
}

/// Names seen outside the script bodies.
#[derive(Default)]
struct Names {
    /// Names that are read: a symbol with one of these names counts as used.
    read: FxHashSet<String>,
    /// Names the template assigns with `name = value`.
    assigned: FxHashSet<String>,
}

/// Every name the template, and the `<script>` open tags, could be referring to.
fn template_names(ctx: &LintContext<'_>) -> Names {
    let mut names = Names::default();
    let source = ctx.source;

    // `<script generics="T extends Item">` refers to types by name.
    for script in ctx.ast.instance.iter().chain(ctx.ast.module.iter()) {
        if let Some(open_tag) =
            source.get(script.span.start as usize..script.content_span.start as usize)
        {
            scan_identifiers(open_tag, &mut names);
        }
    }

    walk_template_nodes(&ctx.ast.html, &mut |node| match node {
        TemplateNode::Element(element) => {
            // `<Component>`, `<Namespace.Component>` and `<object.member>`.
            let first = element.name.split('.').next().unwrap_or("");
            if element.name.contains('.') || first.chars().next().is_some_and(char::is_uppercase) {
                insert_name(&mut names, first);
            }
            for attribute in &element.attributes {
                if let Attribute::Directive {
                    kind, name, value, ..
                } = attribute
                {
                    let subject = name.split('.').next().unwrap_or("");
                    match kind {
                        // `use:action`, `transition:fade`, `animate:flip` name a binding.
                        DirectiveKind::Use
                        | DirectiveKind::Transition
                        | DirectiveKind::In
                        | DirectiveKind::Out
                        | DirectiveKind::Animate => insert_name(&mut names, subject),
                        // `bind:value`, `class:active` and `style:color` are shorthand for
                        // a binding of the same name.
                        DirectiveKind::Binding
                        | DirectiveKind::Class
                        | DirectiveKind::StyleDirective => {
                            if matches!(value, AttributeValue::True) {
                                insert_name(&mut names, subject);
                            }
                        }
                        DirectiveKind::EventHandler | DirectiveKind::Let => {}
                    }
                }
            }
            // Every `{...}` in the open tag: attribute values, shorthand, spreads, attachments.
            let end = (element.start_tag_end as usize + 1).min(source.len());
            if let Some(open_tag) = source.get(element.span.start as usize..end) {
                scan_braces(open_tag, &mut names);
            }
        }
        TemplateNode::MustacheTag(tag) => scan_identifiers(&tag.expression, &mut names),
        TemplateNode::RawMustacheTag(tag) => scan_identifiers(&tag.expression, &mut names),
        TemplateNode::RenderTag(tag) => scan_identifiers(&tag.expression, &mut names),
        TemplateNode::ConstTag(tag) => scan_identifiers(&tag.declaration, &mut names),
        TemplateNode::DebugTag(tag) => {
            for identifier in &tag.identifiers {
                insert_name(&mut names, identifier);
            }
        }
        TemplateNode::IfBlock(block) => scan_identifiers(&block.test, &mut names),
        TemplateNode::EachBlock(block) => {
            scan_identifiers(&block.expression, &mut names);
            scan_identifiers(&block.context, &mut names);
            if let Some(key) = &block.key {
                scan_identifiers(key, &mut names);
            }
        }
        TemplateNode::AwaitBlock(block) => scan_identifiers(&block.expression, &mut names),
        TemplateNode::KeyBlock(block) => scan_identifiers(&block.expression, &mut names),
        TemplateNode::SnippetBlock(block) => scan_identifiers(&block.params, &mut names),
        TemplateNode::Text(_) | TemplateNode::Comment(_) => {}
    });
    names
}

/// Record a name, and the store behind a `$store` subscription.
fn insert_name(names: &mut Names, name: &str) {
    if name.is_empty() {
        return;
    }
    names.read.insert(name.to_string());
    if let Some(store) = name.strip_prefix('$') {
        if !store.is_empty() {
            names.read.insert(store.to_string());
        }
    }
}

/// Scan the contents of every top-level `{...}` in a start tag.
fn scan_braces(text: &str, names: &mut Names) {
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'{' => {
                if depth == 0 {
                    start = index + 1;
                }
                depth += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    scan_identifiers(&text[start..index], names);
                }
            }
            _ => {}
        }
    }
    // An unbalanced tag: scan what is left so nothing is missed.
    if depth > 0 {
        scan_identifiers(&text[start..], names);
    }
}

/// Collect identifiers from expression text. Member names after a `.` and plain assignment
/// targets are skipped. String contents are not, so the result errs towards "used".
fn scan_identifiers(text: &str, names: &mut Names) {
    let bytes = text.as_bytes();
    let is_start = |b: u8| b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b >= 0x80;
    let is_part = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80;
    let mut index = 0;
    while index < bytes.len() {
        if !is_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_part(bytes[index]) {
            index += 1;
        }
        // Skip `object.member`, keep `...spread`.
        let after_dot = start > 0 && bytes[start - 1] == b'.';
        let after_spread = start >= 3 && &bytes[start - 3..start] == b"...";
        // A digit before the name means this is part of a number such as `1e5`.
        let after_digit = start > 0 && bytes[start - 1].is_ascii_digit();
        // `name = value` writes the name without reading it.
        let mut next = index;
        while next < bytes.len() && bytes[next].is_ascii_whitespace() {
            next += 1;
        }
        let assigned = bytes.get(next) == Some(&b'=')
            && !matches!(bytes.get(next + 1), Some(b'=') | Some(b'>'));
        if (after_dot && !after_spread) || after_digit {
            continue;
        }
        if assigned {
            names.assigned.insert(text[start..index].to_string());
        } else {
            insert_name(names, &text[start..index]);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::linter::{Linter, RuleConfig};
    use crate::parser;
    use oxc::allocator::Allocator;

    fn lint_with(source: &str, options: Option<serde_json::Value>) -> Vec<String> {
        let allocator = Allocator::default();
        let result = parser::parse(source, &allocator);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        Linter::all()
            .lint_with_config(
                &result.ast,
                source,
                RuleConfig {
                    options,
                    ..Default::default()
                },
            )
            .into_iter()
            .filter(|diag| diag.rule_name == "@typescript-eslint/no-unused-vars")
            .map(|diag| {
                format!(
                    "{} @ {}",
                    diag.message,
                    &source[diag.span.start as usize..diag.span.end as usize]
                )
            })
            .collect()
    }

    fn lint(source: &str) -> Vec<String> {
        lint_with(source, None)
    }

    #[test]
    fn reports_names_neither_the_script_nor_the_template_uses() {
        let source = r#"<script lang="ts">
    import { onMount, tick } from 'svelte';
    import Used from './Used.svelte';
    import Unused from './Unused.svelte';
    let shown = $state(1);
    let hidden = $state(2);
    let neverAssigned: number;
    function handler() {}
    function orphan() {}
    tick();
</script>

<Used onclick={handler}>{shown}</Used>"#;
        assert_eq!(
            lint(source),
            [
                "'onMount' is defined but never used. @ onMount",
                "'Unused' is defined but never used. @ Unused",
                "'hidden' is assigned a value but never used. @ hidden",
                "'neverAssigned' is defined but never used. @ neverAssigned",
                "'orphan' is defined but never used. @ orphan",
            ]
        );
    }

    #[test]
    fn template_usage_of_every_kind_counts() {
        let source = r#"<script lang="ts">
    import Panel from './Panel.svelte';
    import * as Icons from './icons';
    import { tooltip } from './actions';
    import { fade } from 'svelte/transition';
    import { count } from './stores';
    import type { Item } from './types';
    let items: Item[] = $state([]);
    let value = $state('');
    let active = $state(false);
    let color = $state('red');
    let element: HTMLElement;
    let rest = { id: 'a' };
    let label = 'x';
    let key = 1;
    let promise = Promise.resolve(1);
    const row = (n: number) => n;
    let html = '<b>x</b>';
    let title = 'heading';
</script>

<Panel {...rest} {label} bind:this={element}>
    <Icons.Close />
    <input bind:value class:active style:color use:tooltip transition:fade />
    {#each items as item (item.id)}{item.name}{/each}
    {#key key}{$count}{/key}
    {#await promise then result}{result}{/await}
    {@render row(1)}
    {@html html}
    <p title="A {title} here">text</p>
</Panel>"#;
        assert_eq!(lint(source), Vec::<String>::new());
    }

    #[test]
    fn a_write_alone_is_not_a_use_and_is_where_the_report_goes() {
        let source = r#"<script lang="ts">
    let loading = $state(false);
    function load() {
        loading = true;
    }
    let total = 0;
    total = 5;
</script>

<button onclick={load}>go</button>"#;
        assert_eq!(
            lint(source),
            [
                "'loading' is assigned a value but never used. @ loading",
                "'total' is assigned a value but never used. @ total",
            ]
        );
        // `total` is reported at its last assignment in the same function.
        let allocator = Allocator::default();
        let result = parser::parse(source, &allocator);
        let diags = Linter::all().lint(&result.ast, source);
        let total = diags
            .iter()
            .find(|d| d.message.starts_with("'total'"))
            .unwrap();
        assert_eq!(total.span.start as usize, source.find("total = 5").unwrap());
    }

    #[test]
    fn parameters_follow_after_used_and_exports_are_left_alone() {
        let source = r#"<script lang="ts">
    export let legacyProp = 1;
    export function api(unusedArg: number) {}
    function pair(first: number, second: number) {
        return second;
    }
    function tail(first: number, second: number) {
        return first;
    }
    const callback: (event: Event) => void = () => {};
    try {
        pair(1, 2);
        tail(1, 2);
        callback(new Event('x'));
    } catch (error) {}
</script>"#;
        assert_eq!(
            lint(source),
            [
                "'unusedArg' is defined but never used. @ unusedArg",
                "'second' is defined but never used. @ second",
                "'error' is defined but never used. @ error",
            ]
        );
    }

    #[test]
    fn ignore_patterns_apply_by_kind_and_show_in_the_message() {
        let source = r#"<script lang="ts">
    const _skipped = 1;
    const kept = 2;
    const [_first, second] = [1, 2];
    function run(_arg: number, other: number) {}
    try {
        run(1, 2);
    } catch (_error) {}
</script>"#;
        let options = serde_json::json!([{
            "argsIgnorePattern": "^_",
            "varsIgnorePattern": "^_",
            "caughtErrorsIgnorePattern": "^_",
            "destructuredArrayIgnorePattern": "^_"
        }]);
        assert_eq!(
            lint_with(source, Some(options)),
            [
                "'kept' is assigned a value but never used. Allowed unused vars must match /^_/u. @ kept",
                "'second' is assigned a value but never used. Allowed unused elements of array destructuring must match /^_/u. @ second",
                "'other' is defined but never used. Allowed unused args must match /^_/u. @ other",
            ]
        );
    }

    #[test]
    fn module_script_names_used_by_the_instance_script_or_generics_count() {
        let source = r#"<script lang="ts" module>
    import type { Shape } from './types';
    const shared = 1;
    const lonely = 2;
</script>

<script lang="ts" generics="T extends Shape">
    let { item }: { item: T } = $props();
    console.log(shared, item);
</script>"#;
        assert_eq!(
            lint(source),
            ["'lonely' is assigned a value but never used. @ lonely"]
        );
    }

    #[test]
    fn bindable_props_and_template_assignments() {
        let source = r#"<script lang="ts">
    let { open = $bindable(false), label = 'x', unused = 1 }: Props = $props();
    let picked: string | undefined;
    let shown = $state(false);
    function close() {
        open = false;
    }
</script>

<button onclick={() => (picked = label)} onkeydown={close}>pick</button>
<button onclick={() => (shown = !shown)}>toggle</button>"#;
        assert_eq!(
            lint(source),
            [
                "'unused' is assigned a value but never used. @ unused",
                "'picked' is assigned a value but never used. @ picked",
            ]
        );
    }

    #[test]
    fn updating_a_value_in_place_is_not_a_use() {
        let source = r#"<script lang="ts">
    let offset = 0;
    let visits = 0;
    let kept = 0;
    let flag = false;
    function run(line: string) {
        offset += line.length;
        visits++;
        flag ||= true;
        return (kept += 1);
    }
    run('a');
</script>"#;
        assert_eq!(
            lint(source),
            [
                "'offset' is assigned a value but never used. @ offset",
                "'visits' is assigned a value but never used. @ visits",
            ]
        );
    }

    #[test]
    fn the_config_switches_the_rule_on_without_all_rules() {
        let name = "@typescript-eslint/no-unused-vars";
        let has_rule = |linter: &Linter| linter.rules().iter().any(|rule| rule.name() == name);
        let mut linter = Linter::recommended();
        assert!(!has_rule(&linter));

        let off =
            crate::config::OxvelteConfig::parse(&format!(r#"{{"rules":{{"{name}":"off"}}}}"#))
                .unwrap();
        linter.add_enabled_rules(&off);
        assert!(!has_rule(&linter));

        let on =
            crate::config::OxvelteConfig::parse(&format!(r#"{{"rules":{{"{name}":"error"}}}}"#))
                .unwrap();
        linter.add_enabled_rules(&on);
        linter.add_enabled_rules(&on);
        assert_eq!(
            linter
                .rules()
                .iter()
                .filter(|rule| rule.name() == name)
                .count(),
            1
        );
    }
}
