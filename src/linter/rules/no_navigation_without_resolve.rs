//! `svelte/no-navigation-without-resolve` — disallow SvelteKit navigation calls
//! (`goto`, `pushState`, etc.) without using `$app/paths` `resolveRoute`.
//! ⭐ Recommended

use crate::ast::{Attribute, AttributeValue, TemplateNode};
use crate::linter::{walk_template_nodes, LintContext, Rule};
use oxc::allocator::Allocator;
use oxc::ast::ast::{
    BindingPattern, Expression, ImportDeclarationSpecifier, ModuleExportName, Statement,
    TSSignature, TSType, TSTypeName,
};
use oxc::ast::AstKind;
use oxc::parser::Parser;
use oxc::semantic::Semantic;
use oxc::span::{GetSpan, SourceType, Span};
use rustc_hash::FxHashSet;

const NAV_FUNCTIONS: &[&str] = &["goto", "pushState", "replaceState"];

pub struct NoNavigationWithoutResolve;

impl Rule for NoNavigationWithoutResolve {
    fn name(&self) -> &'static str {
        "svelte/no-navigation-without-resolve"
    }

    fn is_recommended(&self) -> bool {
        true
    }

    fn applies_to_scripts(&self) -> bool {
        true
    }

    fn run<'a>(&self, ctx: &mut LintContext<'a>) {
        if !kit_version_is_eligible(ctx.file_path.as_deref()) {
            return;
        }
        let opts = ctx
            .config
            .options
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first());
        let get_bool = |key: &str| {
            opts.and_then(|v| v.get(key))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        let ignore_goto = get_bool("ignoreGoto");
        let ignore_push_state = get_bool("ignorePushState");
        let ignore_replace_state = get_bool("ignoreReplaceState");
        let ignore_links = get_bool("ignoreLinks");

        // Resolve import locals.
        let mut nav_locals: Vec<(String, &'static str)> = Vec::new(); // (local-callable, original)
        let mut resolve_locals: Vec<String> = Vec::new();

        if let Some(sem) = ctx.instance_semantic {
            for stmt in &sem.nodes().program().body {
                let Statement::ImportDeclaration(imp) = stmt else {
                    continue;
                };
                let src = imp.source.value.as_str();
                let is_nav_mod = src == "$app/navigation";
                let is_paths_mod = src == "$app/paths";
                let Some(specifiers) = &imp.specifiers else {
                    continue;
                };
                for spec in specifiers {
                    match spec {
                        ImportDeclarationSpecifier::ImportSpecifier(s) => {
                            let imported = match &s.imported {
                                ModuleExportName::IdentifierName(n) => n.name.as_str(),
                                ModuleExportName::IdentifierReference(n) => n.name.as_str(),
                                ModuleExportName::StringLiteral(l) => l.value.as_str(),
                            };
                            if is_nav_mod {
                                if let Some(nav) = NAV_FUNCTIONS.iter().find(|f| **f == imported) {
                                    if !is_nav_ignored(
                                        nav,
                                        ignore_goto,
                                        ignore_push_state,
                                        ignore_replace_state,
                                    ) {
                                        nav_locals.push((s.local.name.to_string(), nav));
                                    }
                                }
                            }
                            if is_paths_mod && matches!(imported, "resolve" | "asset") {
                                resolve_locals.push(s.local.name.to_string());
                            }
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                            if is_nav_mod {
                                for nav in NAV_FUNCTIONS {
                                    if is_nav_ignored(
                                        nav,
                                        ignore_goto,
                                        ignore_push_state,
                                        ignore_replace_state,
                                    ) {
                                        continue;
                                    }
                                    nav_locals.push((format!("{}.{}", s.local.name, nav), nav));
                                }
                            }
                            if is_paths_mod {
                                resolve_locals.push(format!("{}.resolve", s.local.name));
                                resolve_locals.push(format!("{}.asset", s.local.name));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Walk script nav calls.
        if !nav_locals.is_empty() {
            if let Some(sem) = ctx.instance_semantic {
                let content_offset = ctx.ast.instance.as_ref().unwrap().content_span.start;
                for node in sem.nodes().iter() {
                    let AstKind::CallExpression(ce) = node.kind() else {
                        continue;
                    };
                    let Some(callee_text) = callee_static_name(&ce.callee) else {
                        continue;
                    };
                    let Some((_, orig_name)) = nav_locals.iter().find(|(l, _)| l == &callee_text)
                    else {
                        continue;
                    };
                    let Some(first_arg) = ce.arguments.first().and_then(|a| a.as_expression())
                    else {
                        continue;
                    };

                    let safe = is_safe_nav_arg(
                        first_arg,
                        &resolve_locals,
                        sem,
                        &mut FxHashSet::default(),
                        *orig_name != "goto",
                    );
                    if !safe {
                        let argument_span = first_arg.span();
                        let s = content_offset + argument_span.start;
                        let e = content_offset + argument_span.end;
                        ctx.diagnostic(
                            format!("Unexpected {}() call without resolve().", orig_name),
                            Span::new(s, e),
                        );
                    }
                }
            }
        }

        if ignore_links {
            return;
        }

        // Link checks also apply when a file imports only types or components.
        walk_template_nodes(&ctx.ast.html, &mut |node| {
            if let TemplateNode::Element(el) = node {
                if el.name != "a" {
                    return;
                }
                // `rel="external"` opts-out. For expression values, parse and
                // look for a literal "external" anywhere in the expression.
                let has_external = el.attributes.iter().any(|a| matches!(
                    a,
                    Attribute::NormalAttribute { name, value, .. } if name == "rel" && match value {
                        AttributeValue::Static(v) => v.split_ascii_whitespace().any(|t| t == "external"),
                        AttributeValue::Expression(e) => expr_contains_literal(e, "external", ctx.instance_semantic),
                        _ => false,
                    }
                ));
                if has_external {
                    return;
                }

                for attr in &el.attributes {
                    let Attribute::NormalAttribute {
                        name, value, span, ..
                    } = attr
                    else {
                        continue;
                    };
                    if name != "href" {
                        continue;
                    }

                    let ok = match value {
                        AttributeValue::Static(v) => is_exempt_href(v),
                        AttributeValue::Expression(expr_text) => {
                            is_safe_template_expr(expr_text, &resolve_locals, ctx.instance_semantic)
                        }
                        AttributeValue::True => true,
                        AttributeValue::Concat(_) => true,
                    };
                    if !ok {
                        ctx.diagnostic("Unexpected href link without resolve().", *span);
                    }
                }
            }
        });
    }
}

/// Upstream's rule conditions accept Kit 1/2. Prefer installed dependencies,
/// then a declared major for fixtures and projects without node_modules.
fn kit_version_is_eligible(filename: Option<&str>) -> bool {
    let Some(filename) = filename else {
        return true;
    };
    let mut directory = std::path::Path::new(filename).parent();
    while let Some(dir) = directory {
        for (path, installed) in [
            (dir.join("node_modules/@sveltejs/kit/package.json"), true),
            (dir.join("package.json"), false),
        ] {
            let Some(package) = std::fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            else {
                continue;
            };
            let version = if installed {
                package.get("version")
            } else {
                ["dependencies", "devDependencies", "peerDependencies"]
                    .iter()
                    .find_map(|section| package.get(section)?.get("@sveltejs/kit"))
            };
            if let Some(major) = version.and_then(|v| v.as_str()).and_then(|s| {
                s.trim_start_matches(['^', '~', '=', ' '])
                    .split('.')
                    .next()?
                    .parse::<u32>()
                    .ok()
            }) {
                return major == 1 || major == 2;
            }
        }
        directory = dir.parent();
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::linter::{LintDiagnostic, Linter};
    use crate::parser;
    use oxc::allocator::Allocator;

    fn lint(source: &str) -> Vec<LintDiagnostic> {
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint(&parsed.ast, source)
            .into_iter()
            .filter(|d| d.rule_name == "svelte/no-navigation-without-resolve")
            .collect()
    }

    #[test]
    fn navigation_reports_the_first_argument() {
        let source = r#"<!-- 😀 -->
<script data-note=">">
import { goto as go, pushState } from '$app/navigation';
import * as nav from '$app/navigation';
go('/jobs');
pushState(`/jobs/${id}`, {});
nav.replaceState(url, {});
</script>"#;
        let diagnostics = lint(source);
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| &source[d.span.start as usize..d.span.end as usize])
                .collect::<Vec<_>>(),
            ["'/jobs'", "`/jobs/${id}`", "url"]
        );
    }

    #[test]
    fn nullish_values_are_safe_but_string_interpolation_is_not() {
        let source = r#"<script lang="ts">
            interface Props { missing: undefined; empty: null; }
            const { missing, empty }: Props = $props();
        </script>
        <a href={missing}>missing</a><a href={empty}>empty</a>
        <a href={`${undefined}`}>string</a><a href={`${null}`}>string</a>
        <a href={`custom:${missing}`}>absolute</a>"#;
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 2);
        for diagnostic in diagnostics {
            let attribute = &source[diagnostic.span.start as usize..diagnostic.span.end as usize];
            assert!(attribute == "href={`${undefined}`}" || attribute == "href={`${null}`}");
        }
    }

    #[test]
    fn branches_must_each_meet_the_navigation_policy() {
        let source = r#"<script>
            import { goto, pushState } from '$app/navigation';
            import { resolve } from '$app/paths';
            const resolved = resolve('/jobs');
            goto(flag ? resolved : resolved);
            goto(flag ? resolved : '/jobs');
            goto(flag ? resolved : '');
            pushState(flag ? resolved : '', {});
            pushState(flag ? resolved : 'https://example.com', {});
        </script>
        <a href={flag ? resolved : '#jobs'}>safe</a>
        <a href={flag ? resolved : '/jobs'}>unsafe</a>
        <a href={'#jobs' - 1}>unsafe operator</a>"#;
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 5);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.message.contains("href"))
                .count(),
            2
        );
    }

    #[test]
    fn pathname_types_follow_import_identity_and_scope() {
        let source = r#"<script lang="ts">
            import { goto, pushState } from '$app/navigation';
            import type { ResolvedPathname as Resolved, Pathname } from '$app/types';
            import type { ResolvedPathname as Other } from './other';
            type Alias = Resolved;
            function accepted(href: Alias) { goto(href); }
            function wrong(href: Other) { goto(href); }
            function unresolved(href: Pathname) { goto(href); }
            function shadowed() {
                type Resolved = string;
                function local(href: Resolved) { goto(href); }
            }
            interface Props { good: Resolved; maybe?: Resolved; }
            const { good, maybe }: Props = $props();
            goto(good);
            goto(maybe);
            pushState(maybe, {});
        </script>
        <a href={good}>safe</a><a href={maybe}>safe</a>"#;
        let diagnostics = lint(source);
        assert_eq!(diagnostics.len(), 5);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|d| d.message.contains("pushState"))
                .count(),
            1
        );
        assert!(diagnostics.iter().all(|d| !d.message.contains("href")));
    }

    #[test]
    fn nullable_pathname_types_are_allowed_only_for_links() {
        let source = r#"<script lang="ts">
            import { goto } from '$app/navigation';
            import type { ResolvedPathname } from '$app/types';
            let href: ResolvedPathname | null = null;
            goto(href);
        </script><a {href}>safe</a>"#;
        assert_eq!(lint(source).len(), 1);
    }
}

fn is_nav_ignored(
    name: &str,
    ignore_goto: bool,
    ignore_push_state: bool,
    ignore_replace_state: bool,
) -> bool {
    match name {
        "goto" => ignore_goto,
        "pushState" => ignore_push_state,
        "replaceState" => ignore_replace_state,
        _ => false,
    }
}

fn is_exempt_href(s: &str) -> bool {
    s.is_empty() || is_absolute_url(s) || s.starts_with("//") || s.starts_with('#')
}

fn is_absolute_url(s: &str) -> bool {
    s.split_once(':')
        .is_some_and(|(scheme, _)| scheme.bytes().all(|b| b.is_ascii_alphabetic() || b == b'+'))
}

/// Compute the longest static string that `expr` is guaranteed to start with,
/// folding `+` concatenation and template-literal quasis. Returns `None` for
/// dynamic expressions whose leading bytes aren't statically known.
fn static_string_prefix(expr: &Expression<'_>) -> Option<String> {
    match expr {
        Expression::StringLiteral(l) => Some(l.value.to_string()),
        Expression::TemplateLiteral(t) => {
            // Upstream treats a scheme in any quasi as an absolute URL.
            if let Some(quasi) = t
                .quasis
                .iter()
                .find(|q| is_absolute_url(q.value.raw.as_str()))
            {
                return Some(quasi.value.raw.to_string());
            }
            let first = t.quasis.first()?;
            let prefix = first
                .value
                .cooked
                .as_deref()
                .unwrap_or(first.value.raw.as_str())
                .to_string();
            // An empty leading quasi says nothing about an interpolated value.
            if prefix.is_empty() && !t.expressions.is_empty() {
                None
            } else {
                Some(prefix)
            }
        }
        Expression::BinaryExpression(b)
            if b.operator == oxc::syntax::operator::BinaryOperator::Addition =>
        {
            let left = static_string_prefix(&b.left)?;
            // If left is a complete static string (no dynamic tail), try to
            // extend with right; otherwise left's prefix is already the answer.
            if matches!(&b.left, Expression::StringLiteral(_)) {
                if let Some(right) = static_string_prefix(&b.right) {
                    return Some(format!("{}{}", left, right));
                }
            }
            Some(left)
        }
        _ => None,
    }
}

/// Parse `expr_text` and return true if any string literal (plain or template
/// quasi) in the expression contains `needle` as a whitespace-separated token.
/// Resolves identifier references to their `const`/`let` initializers via the
/// instance `semantic` when available.
fn expr_contains_literal<'a>(
    expr_text: &str,
    needle: &str,
    instance_sem: Option<&'a Semantic<'a>>,
) -> bool {
    let alloc = Allocator::default();
    let Ok(expr) = Parser::new(&alloc, expr_text, SourceType::mjs()).parse_expression() else {
        return false;
    };
    fn token_match(s: &str, needle: &str) -> bool {
        s.split_ascii_whitespace().any(|t| t == needle)
    }
    fn walk<'a>(
        expr: &Expression<'_>,
        needle: &str,
        sem: Option<&'a Semantic<'a>>,
        seen: &mut FxHashSet<String>,
    ) -> bool {
        match expr {
            Expression::StringLiteral(l) => token_match(l.value.as_str(), needle),
            Expression::TemplateLiteral(t) => t.quasis.iter().any(|q| {
                token_match(
                    q.value.cooked.as_deref().unwrap_or(q.value.raw.as_str()),
                    needle,
                )
            }),
            Expression::BinaryExpression(b) => {
                walk(&b.left, needle, sem, seen) || walk(&b.right, needle, sem, seen)
            }
            Expression::ConditionalExpression(c) => {
                walk(&c.consequent, needle, sem, seen) || walk(&c.alternate, needle, sem, seen)
            }
            Expression::LogicalExpression(l) => {
                walk(&l.left, needle, sem, seen) || walk(&l.right, needle, sem, seen)
            }
            Expression::Identifier(id) => {
                let Some(sem) = sem else { return false };
                let name = id.name.as_str();
                if !seen.insert(name.to_string()) {
                    return false;
                }
                let scoping = sem.scoping();
                let Some(sid) = scoping.find_binding(scoping.root_scope_id(), name.into()) else {
                    return false;
                };
                let decl_node_id = scoping.symbol_declaration(sid);
                let init = std::iter::once(decl_node_id)
                    .chain(sem.nodes().ancestor_ids(decl_node_id))
                    .find_map(|aid| match sem.nodes().kind(aid) {
                        AstKind::VariableDeclarator(vd) => vd.init.as_ref(),
                        _ => None,
                    });
                init.is_some_and(|e| walk(e, needle, Some(sem), seen))
            }
            _ => false,
        }
    }
    walk(&expr, needle, instance_sem, &mut FxHashSet::default())
}

fn callee_static_name(callee: &Expression<'_>) -> Option<String> {
    match callee {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(mem) => {
            if let Expression::Identifier(id) = &mem.object {
                Some(format!("{}.{}", id.name, mem.property.name))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Is this a call to resolve/asset (or aliased/namespaced variant)?
fn is_resolve_call(expr: &Expression<'_>, resolve_locals: &[String]) -> bool {
    let Expression::CallExpression(ce) = expr else {
        return false;
    };
    let Some(text) = callee_static_name(&ce.callee) else {
        return false;
    };
    resolve_locals.iter().any(|r| r == &text)
}

/// Script-side: is the nav-call's first argument safe?
fn is_safe_nav_arg<'a>(
    expr: &'a Expression<'a>,
    resolve_locals: &[String],
    semantic: &'a Semantic<'a>,
    seen: &mut FxHashSet<oxc::semantic::SymbolId>,
    allow_empty: bool,
) -> bool {
    if allow_empty && matches!(expr, Expression::StringLiteral(l) if l.value.is_empty())
        || allow_empty
            && matches!(expr, Expression::TemplateLiteral(t) if t.expressions.is_empty() && t.quasis.iter().all(|q| q.value.raw.is_empty()))
    {
        return true;
    }
    match expr {
        Expression::ConditionalExpression(c) => {
            is_safe_nav_arg(
                &c.consequent,
                resolve_locals,
                semantic,
                &mut seen.clone(),
                allow_empty,
            ) && is_safe_nav_arg(
                &c.alternate,
                resolve_locals,
                semantic,
                &mut seen.clone(),
                allow_empty,
            )
        }
        Expression::CallExpression(_) => is_resolve_call(expr, resolve_locals),
        Expression::NullLiteral(_) => false,
        Expression::Identifier(id) => {
            if id.name == "undefined" {
                return false;
            }
            let reference = semantic.scoping().get_reference(id.reference_id());
            let Some(sid) = reference.symbol_id() else {
                return false;
            };
            if !seen.insert(sid) {
                return false; // recursion guard
            }
            if symbol_has_allowed_type(semantic, sid, false) {
                return true;
            }
            // Find the symbol's initializer.
            let decl_node_id = semantic.scoping().symbol_declaration(sid);
            let init = std::iter::once(decl_node_id)
                .chain(semantic.nodes().ancestor_ids(decl_node_id))
                .find_map(|aid| match semantic.nodes().kind(aid) {
                    AstKind::VariableDeclarator(vd) => vd.init.as_ref(),
                    _ => None,
                });
            match init {
                Some(init_expr) => {
                    is_safe_nav_arg(init_expr, resolve_locals, semantic, seen, allow_empty)
                }
                None => false,
            }
        }
        _ => false,
    }
}

/// Parse a template-expression text and classify it as safe or not.
fn is_safe_template_expr<'a>(
    expr_text: &str,
    resolve_locals: &[String],
    instance_sem: Option<&'a Semantic<'a>>,
) -> bool {
    let alloc = Allocator::default();
    let parsed = Parser::new(&alloc, expr_text, SourceType::mjs()).parse_expression();
    let Ok(expr) = parsed else {
        // Fallback: lenient — don't flag if we can't parse.
        return true;
    };
    let mut seen = FxHashSet::default();
    is_safe_template_root(&expr, resolve_locals, instance_sem, &mut seen)
}

/// Top-level safety check for a template-attribute expression. Differs from
/// `is_safe_nav_arg` only slightly: for Identifier refs, we look up the
/// declaration in the instance script's semantic model.
fn is_safe_template_root<'a>(
    expr: &Expression<'_>,
    resolve_locals: &[String],
    instance_sem: Option<&'a Semantic<'a>>,
    seen: &mut FxHashSet<String>,
) -> bool {
    if static_string_prefix(expr).is_some_and(|p| is_exempt_href(&p)) {
        return true;
    }
    match expr {
        Expression::ConditionalExpression(c) => {
            is_safe_template_root(
                &c.consequent,
                resolve_locals,
                instance_sem,
                &mut seen.clone(),
            ) && is_safe_template_root(
                &c.alternate,
                resolve_locals,
                instance_sem,
                &mut seen.clone(),
            )
        }
        Expression::CallExpression(_) => is_resolve_call(expr, resolve_locals),
        Expression::NullLiteral(_) => true,
        Expression::Identifier(id) => {
            if id.name == "undefined" {
                return true;
            }
            // The parsed expression's Identifier doesn't have a reference_id
            // resolved against our instance semantic. Resolve by NAME in root scope.
            let name = id.name.as_str();
            if !seen.insert(name.to_string()) {
                return false;
            }
            let Some(sem) = instance_sem else {
                return false;
            };
            let scoping = sem.scoping();
            let Some(sid) = scoping.find_binding(scoping.root_scope_id(), name.into()) else {
                return false;
            };
            if symbol_has_allowed_type(sem, sid, true) {
                return true;
            }
            let decl_node_id = scoping.symbol_declaration(sid);
            let init = std::iter::once(decl_node_id)
                .chain(sem.nodes().ancestor_ids(decl_node_id))
                .find_map(|aid| match sem.nodes().kind(aid) {
                    AstKind::VariableDeclarator(vd) => vd.init.as_ref(),
                    _ => None,
                });
            match init {
                Some(init_expr) => is_safe_instance_expr(init_expr, resolve_locals, sem, seen),
                None => false,
            }
        }
        // `{foo ?? '/bar'}`, etc. — conservative: flag.
        _ => false,
    }
}

fn is_nullish_type(ty: &TSType<'_>) -> bool {
    matches!(ty, TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_))
}

/// Get the annotation on a binding, including a property destructured from a
/// typed object. Type references are resolved through their semantic symbols.
fn symbol_type<'a>(
    sem: &'a Semantic<'a>,
    sid: oxc::semantic::SymbolId,
) -> Option<(&'a TSType<'a>, bool)> {
    let declaration = sem.scoping().symbol_declaration(sid);
    let (pattern, annotation, optional) = std::iter::once(declaration)
        .chain(sem.nodes().ancestor_ids(declaration))
        .find_map(|id| match sem.nodes().kind(id) {
            AstKind::VariableDeclarator(v) => Some((&v.id, v.type_annotation.as_ref(), false)),
            AstKind::FormalParameter(p) => {
                Some((&p.pattern, p.type_annotation.as_ref(), p.optional))
            }
            _ => None,
        })?;
    let ty = &annotation?.type_annotation;
    let BindingPattern::ObjectPattern(pattern) = pattern else {
        return Some((ty, optional));
    };
    let property = pattern.properties.iter().find(|p| matches!(&p.value, BindingPattern::BindingIdentifier(id) if id.symbol_id.get() == Some(sid)))?;
    let name = property.key.static_name()?;
    let members = match ty {
        TSType::TSTypeLiteral(literal) => &literal.members,
        TSType::TSTypeReference(reference) => {
            let TSTypeName::IdentifierReference(id) = &reference.type_name else {
                return None;
            };
            let type_sid = sem.scoping().get_reference(id.reference_id()).symbol_id()?;
            let AstKind::TSInterfaceDeclaration(interface) =
                sem.nodes().kind(sem.scoping().symbol_declaration(type_sid))
            else {
                return None;
            };
            &interface.body.body
        }
        _ => return None,
    };
    members.iter().find_map(|member| {
        let TSSignature::TSPropertySignature(p) = member else {
            return None;
        };
        if p.key.static_name().as_deref() != Some(name.as_ref()) {
            return None;
        }
        Some((&p.type_annotation.as_ref()?.type_annotation, p.optional))
    })
}

fn symbol_has_allowed_type(
    sem: &Semantic<'_>,
    sid: oxc::semantic::SymbolId,
    allow_nullish: bool,
) -> bool {
    symbol_type(sem, sid).is_some_and(|(ty, optional)| {
        (!optional || allow_nullish)
            && is_allowed_type(ty, sem, allow_nullish, &mut FxHashSet::default())
    })
}

fn is_allowed_type(
    ty: &TSType<'_>,
    sem: &Semantic<'_>,
    allow_nullish: bool,
    seen: &mut FxHashSet<oxc::semantic::SymbolId>,
) -> bool {
    if allow_nullish && is_nullish_type(ty) {
        return true;
    }
    match ty {
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .all(|ty| is_allowed_type(ty, sem, allow_nullish, &mut seen.clone())),
        TSType::TSTypeReference(reference) => {
            let TSTypeName::IdentifierReference(id) = &reference.type_name else {
                return false;
            };
            let Some(sid) = sem.scoping().get_reference(id.reference_id()).symbol_id() else {
                return false;
            };
            if !seen.insert(sid) {
                return false;
            }
            let declaration = sem.scoping().symbol_declaration(sid);
            for node in std::iter::once(declaration).chain(sem.nodes().ancestor_ids(declaration)) {
                match sem.nodes().kind(node) {
                    AstKind::TSTypeAliasDeclaration(alias) => {
                        return is_allowed_type(&alias.type_annotation, sem, allow_nullish, seen)
                    }
                    AstKind::ImportDeclaration(import) if import.source.value == "$app/types" => {
                        return import.specifiers.as_ref().is_some_and(|specifiers| specifiers.iter().any(|specifier| {
                            matches!(specifier, ImportDeclarationSpecifier::ImportSpecifier(s) if s.local.symbol_id.get() == Some(sid) && s.imported.name() == "ResolvedPathname")
                        }));
                    }
                    _ => {}
                }
            }
            false
        }
        _ => false,
    }
}

/// Safety check for an expression in the instance script (uses instance semantic
/// for reference resolution).
fn is_safe_instance_expr<'a>(
    expr: &'a Expression<'a>,
    resolve_locals: &[String],
    sem: &'a Semantic<'a>,
    seen: &mut FxHashSet<String>,
) -> bool {
    if static_string_prefix(expr).is_some_and(|p| is_exempt_href(&p)) {
        return true;
    }
    match expr {
        Expression::ConditionalExpression(c) => {
            is_safe_instance_expr(&c.consequent, resolve_locals, sem, &mut seen.clone())
                && is_safe_instance_expr(&c.alternate, resolve_locals, sem, &mut seen.clone())
        }
        Expression::CallExpression(_) => is_resolve_call(expr, resolve_locals),
        Expression::NullLiteral(_) => true,
        Expression::Identifier(id) => {
            if id.name == "undefined" {
                return true;
            }
            let name = id.name.as_str();
            if !seen.insert(name.to_string()) {
                return false;
            }
            let reference = sem.scoping().get_reference(id.reference_id());
            let Some(sid) = reference.symbol_id() else {
                return false;
            };
            if symbol_has_allowed_type(sem, sid, true) {
                return true;
            }
            let decl_node_id = sem.scoping().symbol_declaration(sid);
            let init = std::iter::once(decl_node_id)
                .chain(sem.nodes().ancestor_ids(decl_node_id))
                .find_map(|aid| match sem.nodes().kind(aid) {
                    AstKind::VariableDeclarator(vd) => vd.init.as_ref(),
                    _ => None,
                });
            match init {
                Some(init_expr) => is_safe_instance_expr(init_expr, resolve_locals, sem, seen),
                None => false,
            }
        }
        _ => false,
    }
}
