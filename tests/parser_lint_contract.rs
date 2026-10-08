use oxc::allocator::Allocator;
use oxvelte::ast::{Attribute, AttributeValue, TemplateNode};
use oxvelte::parser;

#[test]
fn lint_parsing_leaves_compiler_constraints_to_rules() {
    let sources = [
        "<slot name={name} />",
        "<input bind:value={''} />",
        "<div bind:missing={value} />",
        "<div transition:a transition:b />",
        "<table><tr><td>value</td></tr></table>",
        "<Component use:action />",
        "<svelte:element this={tag} bind:group={values} />",
    ];
    for source in sources {
        let alloc = Allocator::default();
        let strict = parser::parse(source, &alloc);
        assert!(!strict.errors.is_empty(), "strict validation: {source}");
        let lint = parser::parse_for_lint(source, &alloc);
        assert!(lint.errors.is_empty(), "{source}: {:?}", lint.errors);
        assert!(!lint.ast.html.nodes.is_empty(), "retained tree: {source}");
    }
}

#[test]
fn lint_parsing_retains_attributes_and_source_spans() {
    let source = "<slot name={current} />";
    let alloc = Allocator::default();
    let parsed = parser::parse_for_lint(source, &alloc);
    let TemplateNode::Element(slot) = &parsed.ast.html.nodes[0] else {
        panic!("expected a slot element");
    };
    let Attribute::NormalAttribute { name, value, span } = &slot.attributes[0] else {
        panic!("expected a normal attribute");
    };
    assert_eq!(name, "name");
    assert!(matches!(value, AttributeValue::Expression(text) if text == "current"));
    assert_eq!(
        &source[span.start as usize..span.end as usize],
        "name={current}"
    );
    assert!(slot.attribute_meta[0].expression_ast.is_some());
}

#[test]
fn lint_parsing_preserves_structural_syntax_errors() {
    for source in [
        "<div title=\"unfinished>",
        "<div title={value />",
        "<div>",
        "{#if condition}<p>text</p>",
        "{/if}",
        "<script>let value = 1;",
        "{value +}",
        "<div title={value +} />",
        "<div title=\"hello {value +}\" />",
        "{#if value +}text{/if}",
        "{#each values + as value}text{/each}",
        "{#await value +}text{/await}",
        "{#key value +}text{/key}",
        "{@html value +}",
        "{@render value +}",
        "<div {...value +} />",
        "{#if }text{/if}",
    ] {
        let alloc = Allocator::default();
        assert!(
            !parser::parse_for_lint(source, &alloc).errors.is_empty(),
            "syntax error must survive lint parsing: {source}"
        );
    }
}

#[test]
fn lint_expression_errors_use_original_source_positions() {
    let source = "<!-- prefix -->\n<div title={value +} />";
    let alloc = Allocator::default();
    let parsed = parser::parse_for_lint(source, &alloc);
    assert!(!parsed.errors.is_empty());
    let labels = parsed.errors[0].labels.as_ref().unwrap();
    assert_eq!(labels[0].offset(), source.find('}').unwrap());
}

#[test]
fn lint_parsing_accepts_spaced_template_tags_without_regex_recovery() {
    let source = "{ #snippet foo() }{ /snippet }{ @render foo() }";
    let alloc = Allocator::default();
    let parsed = parser::parse_for_lint(source, &alloc);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.ast.html.nodes.len(), 2);
    let TemplateNode::SnippetBlock(snippet) = &parsed.ast.html.nodes[0] else {
        panic!("spaced snippet tags must form a block");
    };
    assert_eq!(snippet.name, "foo");
    assert!(snippet.body.nodes.is_empty());
    assert_eq!(
        &source[snippet.span.start as usize..snippet.span.end as usize],
        "{ #snippet foo() }{ /snippet }"
    );
    let TemplateNode::RenderTag(render) = &parsed.ast.html.nodes[1] else {
        panic!("spaced render tags must retain their expression metadata");
    };
    assert_eq!(
        &source[render.expression_span.start as usize..render.expression_span.end as usize],
        "foo() "
    );
}

#[test]
fn lint_parsing_accepts_else_without_a_test_expression() {
    let source = "{#if condition}yes{:else if other}maybe{:else}no{/if}";
    let alloc = Allocator::default();
    assert!(parser::parse_for_lint(source, &alloc).errors.is_empty());
}

#[test]
fn directive_subjects_keep_dollar_identifiers_and_exact_spans() {
    let source =
        "<div use:$store transition:$store in:$store out:$store animate:$store class:$store />";
    let alloc = Allocator::default();
    let parsed = parser::parse_for_lint(source, &alloc);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let TemplateNode::Element(element) = &parsed.ast.html.nodes[0] else {
        panic!("expected an element");
    };
    assert_eq!(element.attributes.len(), 6);
    for (attribute, meta) in element.attributes.iter().zip(&element.attribute_meta) {
        let Attribute::Directive { name, span, .. } = attribute else {
            panic!("a dollar identifier must remain one directive");
        };
        assert_eq!(name, "$store");
        let subject = meta.directive_subject_span.unwrap();
        assert_eq!(
            &source[subject.start as usize..subject.end as usize],
            "$store"
        );
        assert!(source[span.start as usize..span.end as usize].ends_with("$store"));
    }
    assert!(
        parser::parse("<div use:$action class:$condition />", &alloc)
            .errors
            .is_empty()
    );
}
