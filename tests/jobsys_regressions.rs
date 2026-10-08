use oxc::allocator::Allocator;
use oxvelte::{config::OxvelteConfig, linter::Linter, parser};
use serde_json::{json, Value};

fn check(name: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/jobsys");
    let cases: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("cases.json")).unwrap()).unwrap();
    let case = cases
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    let file = root.join(case["file"].as_str().unwrap());
    let source = std::fs::read_to_string(&file).unwrap();
    let rule = case["rule"].as_str().unwrap();
    let mut lint = Linter::all();
    let mut rules = serde_json::Map::new();
    for r in lint.rules() {
        rules.insert(r.name().into(), json!("off"));
    }
    let mut enabled = vec![json!("error")];
    if let Some(options) = case["options"].as_array() {
        enabled.extend(options.iter().cloned());
    }
    rules.insert(rule.into(), json!(enabled));
    let config = OxvelteConfig::parse(&json!({"rules": rules}).to_string()).unwrap();
    lint.remove_disabled_rules(&config);
    let alloc = Allocator::default();
    let path = file.to_str().unwrap();
    let diagnostics = if path.ends_with(".svelte.ts") {
        lint.lint_svelte_script_with_project_config_and_path(&source, true, &config, path)
    } else {
        let parsed = parser::parse_for_lint(&source, &alloc);
        assert!(parsed.errors.is_empty(), "{}: {:?}", name, parsed.errors);
        lint.lint_with_project_config_and_path(&parsed.ast, &source, &config, path)
    };
    let messages: Vec<_> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    let expected: Vec<_> = case["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(messages, expected, "{name}");
    if let Some(span) = case["spanText"].as_str() {
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            &source[diagnostics[0].span.start as usize..diagnostics[0].span.end as usize],
            span
        );
    }
}

macro_rules! fixture {
    ($test:ident, $case:literal) => {
        #[test]
        fn $test() {
            check($case);
        }
    };
}
fixture!(multiline_unions, "union-property");
fixture!(block_scoped_allow, "block-scoped-function");
fixture!(nested_style, "nested-style");
fixture!(kit2_navigation, "kit2_navigation_svelte");
fixture!(kit3_navigation, "kit3_navigation_svelte");
fixture!(block_scoped_disallow, "inner-disallow_svelte");
fixture!(unused_get_id, "unused-get-id_svelte");
fixture!(destructuring_comment, "destructuring-comment_svelte");
fixture!(destructuring_default, "destructuring-default_svelte");
fixture!(index_signature_boundary, "index-signature-boundary_svelte");
fixture!(nested_array_consumption, "nested-array-consumption_svelte");
fixture!(exported_mutable_set, "exported_mutable_set");
