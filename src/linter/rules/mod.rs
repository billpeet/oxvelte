//! Lint rule implementations.
//!
//! Each rule is in its own submodule. This module provides:
//! - `all_rules()` — every implemented rule
//! - `recommended_rules()` — only rules marked ⭐

mod block_lang;
mod button_has_type;
mod html_self_closing;
mod infinite_reactive_loop;
mod max_lines_per_block;
mod no_add_event_listener;
mod no_at_debug_tags;
mod no_at_html_tags;
mod no_bind_value_on_checkable_inputs;
mod no_dom_manipulating;
mod no_dupe_else_if_blocks;
mod no_dupe_on_directives;
mod no_dupe_style_properties;
mod no_dupe_use_directives;
mod no_export_load_in_svelte_module_in_kit_pages;
mod no_extra_reactive_curlies;
mod no_ignored_unsubscribe;
mod no_immutable_reactive_statements;
mod no_inline_styles;
mod no_inner_declarations;
mod no_inspect;
mod no_not_function_handler;
mod no_object_in_text_mustaches;
mod no_raw_special_elements;
mod no_reactive_functions;
mod no_reactive_literals;
mod no_reactive_reassign;
mod no_restricted_html_elements;
mod no_shorthand_style_property_overrides;
mod no_spaces_around_equal_signs_in_attribute;
mod no_store_async;
mod no_svelte_internal;
mod no_target_blank;
mod no_trailing_spaces;
mod no_unknown_style_directive_property;
mod no_unnecessary_state_wrap;
mod no_unused_props;
mod no_unused_svelte_ignore;
mod no_useless_children_snippet;
mod no_useless_mustaches;
mod prefer_class_directive;
mod prefer_derived_over_derived_by;
mod prefer_style_directive;
mod prefer_writable_derived;
mod require_each_key;
mod require_event_dispatcher_types;
mod require_optimized_style_attribute;
mod require_stores_init;
mod shorthand_attribute;
mod shorthand_directive;
mod spaced_html_comment;
mod style_declarations;
mod valid_each_key;

mod no_dynamic_slot_name;
mod no_goto_without_base;
mod no_navigation_without_base;

// New rules
mod comment_directive;
mod consistent_selector_style;
mod derived_has_same_inputs_outputs;
mod experimental_require_slot_types;
mod experimental_require_strict_events;
mod first_attribute_linebreak;
mod html_closing_bracket_new_line;
mod html_closing_bracket_spacing;
mod html_quotes;
mod max_attributes_per_line;
mod mustache_spacing;
mod no_navigation_without_resolve;
mod no_top_level_browser_globals;
mod no_unused_class_name;
mod prefer_const;
mod prefer_destructured_store_props;
mod prefer_svelte_reactivity;
mod require_event_prefix;
mod require_store_callbacks_use_set_param;
mod require_store_reactive_access;
mod sort_attributes;
mod system;
mod valid_compile;
mod valid_prop_names_in_kit_pages;
mod valid_style_parse;

use super::Rule;
use crate::ast::{AttributeValue, AttributeValuePart};

/// Token boundaries for suggestion edits. Comments are excluded, so punctuation
/// inside a comment cannot be mistaken for a label or call delimiter.
pub(super) fn script_token_spans(source: &str) -> Vec<oxc::span::Span> {
    let allocator = oxc::allocator::Allocator::default();
    let parsed = oxc::parser::Parser::new(&allocator, source, oxc::span::SourceType::ts())
        .with_config(oxc::parser::config::TokensParserConfig)
        .parse();
    parsed
        .tokens
        .iter()
        .filter(|token| token.end() > token.start())
        .map(|token| oxc::span::Span::new(token.start(), token.end()))
        .collect()
}

pub(super) fn directive_expression_key(value: &AttributeValue) -> String {
    match value {
        AttributeValue::True => String::new(),
        AttributeValue::Expression(expr) => canonical_js_expression(expr),
        AttributeValue::Static(value) => format!("static:{value}"),
        AttributeValue::Concat(parts) => {
            let mut key = String::new();
            for part in parts {
                match part {
                    AttributeValuePart::Static(value) => {
                        key.push_str("static:");
                        key.push_str(value);
                    }
                    AttributeValuePart::Expression(expr) => {
                        key.push_str("expr:");
                        key.push_str(&canonical_js_expression(expr));
                    }
                }
                key.push('\0');
            }
            key
        }
    }
}

fn canonical_js_expression(source: &str) -> String {
    // Upstream compares tokens, including literal spelling, without comments.
    // Parsing an expression wrapper lets OXC distinguish regexes, templates and
    // adjacent operators instead of joining whitespace-separated text.
    let allocator = oxc::allocator::Allocator::default();
    let wrapped = format!("({source}\n)");
    let parsed = oxc::parser::Parser::new(&allocator, &wrapped, oxc::span::SourceType::ts())
        .with_config(oxc::parser::config::TokensParserConfig)
        .parse();
    if !parsed.errors.is_empty() {
        return format!("unparsed:{source}");
    }
    let mut out = String::new();
    for token in &parsed.tokens {
        let text = &wrapped[token.start() as usize..token.end() as usize];
        // Length prefixes preserve token boundaries even inside literal text.
        out.push_str(&text.len().to_string());
        out.push(':');
        out.push_str(text);
    }
    out
}

#[cfg(test)]
mod directive_identity_tests {
    use crate::{linter::Linter, parser};
    use oxc::allocator::Allocator;

    #[test]
    fn distinct_operators_and_literal_content_are_not_duplicate_directives() {
        let source = r#"<button on:click={() => x++ + y} on:click={() => x + ++y} />
<div use:foo={"a b"} use:foo={"ab"} />
<div use:bar={/a b/} use:bar={/ab/} />
<div use:baz={`a b`} use:baz={`ab`} />"#;
        assert_eq!(duplicates(source), 0);
    }

    #[test]
    fn comments_and_spacing_between_tokens_do_not_change_identity() {
        let source = r#"<button on:click={() => x + y} on:click={() => x /* note */ + y} />
<div use:foo={"a b"} use:foo={ /* note */ "a b"} />
<div use:bar={/a b/} use:bar={ /a b/ } />
<div use:baz={`a ${ x + y }`} use:baz={`a ${x+y}`} />"#;
        assert_eq!(duplicates(source), 8);
    }

    fn duplicates(source: &str) -> usize {
        let allocator = Allocator::default();
        let parsed = parser::parse(source, &allocator);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        Linter::all()
            .lint(&parsed.ast, source)
            .iter()
            .filter(|d| {
                matches!(
                    d.rule_name,
                    "svelte/no-dupe-on-directives" | "svelte/no-dupe-use-directives"
                )
            })
            .count()
    }
}

/// Return all implemented lint rules.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(no_at_html_tags::NoAtHtmlTags),
        Box::new(no_at_debug_tags::NoAtDebugTags),
        Box::new(no_dupe_else_if_blocks::NoDupeElseIfBlocks),
        Box::new(no_dupe_style_properties::NoDupeStyleProperties),
        Box::new(no_dupe_use_directives::NoDupeUseDirectives),
        Box::new(no_dupe_on_directives::NoDupeOnDirectives),
        Box::new(require_each_key::RequireEachKey),
        Box::new(no_object_in_text_mustaches::NoObjectInTextMustaches),
        Box::new(no_useless_mustaches::NoUselessMustaches),
        Box::new(no_target_blank::NoTargetBlank),
        Box::new(button_has_type::ButtonHasType),
        Box::new(no_raw_special_elements::NoRawSpecialElements),
        Box::new(no_inspect::NoInspect),
        Box::new(no_svelte_internal::NoSvelteInternal),
        Box::new(no_inline_styles::NoInlineStyles),
        Box::new(valid_each_key::ValidEachKey),
        Box::new(no_not_function_handler::NoNotFunctionHandler),
        Box::new(no_ignored_unsubscribe::NoIgnoredUnsubscribe),
        Box::new(no_inner_declarations::NoInnerDeclarations),
        Box::new(spaced_html_comment::SpacedHtmlComment),
        Box::new(no_trailing_spaces::NoTrailingSpaces),
        Box::new(require_event_dispatcher_types::RequireEventDispatcherTypes),
        Box::new(no_unused_svelte_ignore::NoUnusedSvelteIgnore),
        Box::new(html_self_closing::HtmlSelfClosing),
        Box::new(no_unknown_style_directive_property::NoUnknownStyleDirectiveProperty),
        Box::new(no_shorthand_style_property_overrides::NoShorthandStylePropertyOverrides),
        Box::new(shorthand_attribute::ShorthandAttribute),
        Box::new(shorthand_directive::ShorthandDirective),
        Box::new(no_reactive_literals::NoReactiveLiterals),
        Box::new(no_reactive_functions::NoReactiveFunctions),
        Box::new(no_useless_children_snippet::NoUselessChildrenSnippet),
        Box::new(no_immutable_reactive_statements::NoImmutableReactiveStatements),
        Box::new(no_dom_manipulating::NoDomManipulating),
        Box::new(no_reactive_reassign::NoReactiveReassign),
        Box::new(no_store_async::NoStoreAsync),
        Box::new(prefer_class_directive::PreferClassDirective),
        Box::new(
            no_export_load_in_svelte_module_in_kit_pages::NoExportLoadInSvelteModuleInKitPages,
        ),
        Box::new(infinite_reactive_loop::InfiniteReactiveLoop),
        Box::new(no_unnecessary_state_wrap::NoUnnecessaryStateWrap),
        Box::new(no_unused_props::NoUnusedProps),
        Box::new(prefer_writable_derived::PreferWritableDerived),
        Box::new(prefer_derived_over_derived_by::PreferDerivedOverDerivedBy),
        Box::new(no_bind_value_on_checkable_inputs::NoBindValueOnCheckableInputs),
        Box::new(require_stores_init::RequireStoresInit),
        Box::new(no_add_event_listener::NoAddEventListener),
        Box::new(block_lang::BlockLang),
        Box::new(max_lines_per_block::MaxLinesPerBlock),
        Box::new(require_optimized_style_attribute::RequireOptimizedStyleAttribute),
        Box::new(prefer_style_directive::PreferStyleDirective),
        Box::new(no_spaces_around_equal_signs_in_attribute::NoSpacesAroundEqualSignsInAttribute),
        Box::new(no_restricted_html_elements::NoRestrictedHtmlElements),
        Box::new(no_extra_reactive_curlies::NoExtraReactiveCurlies),
        // New rules
        Box::new(no_top_level_browser_globals::NoTopLevelBrowserGlobals),
        Box::new(prefer_svelte_reactivity::PreferSvelteReactivity),
        Box::new(require_store_callbacks_use_set_param::RequireStoreCallbacksUseSetParam),
        Box::new(require_store_reactive_access::RequireStoreReactiveAccess),
        Box::new(valid_compile::ValidCompile),
        Box::new(valid_style_parse::ValidStyleParse),
        Box::new(no_unused_class_name::NoUnusedClassName),
        Box::new(prefer_const::PreferConst),
        Box::new(prefer_destructured_store_props::PreferDestructuredStoreProps),
        Box::new(consistent_selector_style::ConsistentSelectorStyle),
        Box::new(derived_has_same_inputs_outputs::DerivedHasSameInputsOutputs),
        Box::new(first_attribute_linebreak::FirstAttributeLinebreak),
        Box::new(html_closing_bracket_new_line::HtmlClosingBracketNewLine),
        Box::new(html_closing_bracket_spacing::HtmlClosingBracketSpacing),
        Box::new(html_quotes::HtmlQuotes),
        Box::new(max_attributes_per_line::MaxAttributesPerLine),
        Box::new(mustache_spacing::MustacheSpacing),
        Box::new(sort_attributes::SortAttributes),
        Box::new(require_event_prefix::RequireEventPrefix),
        Box::new(valid_prop_names_in_kit_pages::ValidPropNamesInKitPages),
        Box::new(no_navigation_without_resolve::NoNavigationWithoutResolve),
        Box::new(experimental_require_slot_types::ExperimentalRequireSlotTypes),
        Box::new(experimental_require_strict_events::ExperimentalRequireStrictEvents),
        Box::new(comment_directive::CommentDirective),
        Box::new(system::System),
        Box::new(no_dynamic_slot_name::NoDynamicSlotName),
        Box::new(no_goto_without_base::NoGotoWithoutBase),
        Box::new(no_navigation_without_base::NoNavigationWithoutBase),
    ]
}

/// Return only recommended (⭐) rules.
pub fn recommended_rules() -> Vec<Box<dyn Rule>> {
    all_rules()
        .into_iter()
        .filter(|r| r.is_recommended())
        .collect()
}
