#[path = "support/parity.rs"]
#[allow(dead_code)]
mod parity;

use oxc::span::Span;
use oxvelte::linter::{Fix, LintContext, LintDiagnostic, Linter, Rule, Suggestion};
use serde_json::json;

fn runner_args(arguments: &[&str]) -> Result<parity::RunnerArgs, String> {
    parity::runner_arguments(arguments.iter().map(|argument| argument.to_string()))
}

#[test]
fn independent_matrix_runs_require_explicit_baseline_handling() {
    let defaults = runner_args(&[]).unwrap();
    assert!(defaults.corpus.is_none() && defaults.baseline.is_none());
    assert!(!defaults.no_baseline && !defaults.strict && !defaults.update);
    let strict = runner_args(&[
        "--corpus",
        "reports/svelte4/corpus",
        "--strict",
        "--no-baseline",
        "--rule",
        "svelte/valid-compile",
    ])
    .unwrap();
    assert_eq!(strict.rule.as_deref(), Some("valid-compile"));
    assert_eq!(
        strict.corpus.unwrap(),
        std::path::PathBuf::from("reports/svelte4/corpus")
    );
    assert!(strict.strict && strict.no_baseline);
    let update = runner_args(&[
        "--corpus",
        "reports/svelte4/corpus",
        "--baseline",
        "reports/svelte4/baseline.json",
        "--update-baseline",
    ])
    .unwrap();
    assert!(update.update);
    assert_eq!(
        update.baseline.unwrap(),
        std::path::PathBuf::from("reports/svelte4/baseline.json")
    );
    for arguments in [
        vec!["--corpus", "reports/svelte4/corpus"],
        vec!["--corpus", "reports/svelte4/corpus", "--update-baseline"],
        vec!["--no-baseline"],
        vec!["--strict", "--no-baseline", "--baseline", "other.json"],
        vec!["--update-baseline", "--rule", "valid-compile"],
        vec!["--update-baseline", "--strict"],
        vec!["--corpus"],
        vec!["--baseline"],
    ] {
        assert!(
            runner_args(&arguments).is_err(),
            "accepted unsafe arguments: {arguments:?}"
        );
    }
}

#[test]
fn type_aware_fixture_configuration_uses_the_imported_suite_project() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/upstream/eslint-plugin-svelte");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
            .unwrap();
    let rule = "@typescript-eslint/no-unnecessary-condition";
    let cases: Vec<_> = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["rule"] == rule)
        .collect();
    assert_eq!(cases.len(), 7);
    let expected = root.join("tests/fixtures/rules/tsconfig.json");
    let project: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&expected).unwrap()).unwrap();
    assert_eq!(project["compilerOptions"]["strict"], true);
    for case in cases {
        let settings = parity::fixture_settings(&case["config"], None, &root, rule).unwrap();
        assert_eq!(
            settings["typescript"]["project"],
            expected.to_string_lossy().replace('\\', "/")
        );
    }
    assert_eq!(
        parity::fixture_settings(&json!({}), None, &root, "require-each-key"),
        None
    );
}

#[test]
fn explicit_typescript_projects_resolve_paths_and_preserve_other_settings() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let absolute = root.join("separate/tsconfig.json");
    let config = json!({
        "settings": {"typescript": {"strictNullChecks": true}, "svelte": {"ignoreWarnings": ["test"]}},
        "languageOptions": {"parserOptions": {"project": ["custom/tsconfig.json", absolute]}}
    });
    let settings = parity::fixture_settings(
        &config,
        None,
        root,
        "@typescript-eslint/no-unnecessary-condition",
    )
    .unwrap();
    assert_eq!(
        settings["typescript"]["project"],
        json!([
            root.join("custom/tsconfig.json")
                .to_string_lossy()
                .replace('\\', "/"),
            absolute.to_string_lossy().replace('\\', "/")
        ])
    );
    assert_eq!(settings["typescript"]["strictNullChecks"], true);
    assert_eq!(settings["svelte"]["ignoreWarnings"], json!(["test"]));
    let disabled = json!({"languageOptions": {"parserOptions": {"project": false}}});
    assert_eq!(
        parity::fixture_settings(
            &disabled,
            None,
            root,
            "@typescript-eslint/no-unnecessary-condition"
        )
        .unwrap()["typescript"]["project"],
        false
    );
}

#[test]
fn compiler_configuration_preserves_settings_and_executable_module_paths() {
    let config = json!({
        "settings": {"svelte": {"ignoreWarnings": ["example"]}, "compiler": {"customElement": true}},
        "languageOptions": {"parserOptions": {
            "parser": "@babel/eslint-parser",
            "svelteConfig": {"compilerOptions": {"experimental": {"async": true}}, "kit": {"files": {"routes": "custom/routes"}}}
        }}
    });
    let settings = parity::compiler_settings(&config, Some("/fixtures/_config.cjs")).unwrap();
    assert_eq!(
        settings["compiler"]["executableConfigPath"],
        "/fixtures/_config.cjs"
    );
    assert_eq!(settings["compiler"]["parser"], "@babel/eslint-parser");
    assert_eq!(settings["compiler"]["customElement"], true);
    assert_eq!(
        settings["compiler"]["svelteConfig"],
        config["languageOptions"]["parserOptions"]["svelteConfig"]
    );
    assert_eq!(
        settings["svelte"]["kit"]["files"]["routes"],
        "custom/routes"
    );
    assert_eq!(settings["svelte"]["ignoreWarnings"], json!(["example"]));
    assert_eq!(parity::compiler_settings(&json!({}), None), None);
}

#[test]
fn script_parse_errors_distinguish_modifier_checks_from_syntax_errors() {
    use oxc::{allocator::Allocator, parser::Parser, span::SourceType};
    let source = "class Box { readonly protected value: number; }";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert_eq!(parsed.program.body.len(), 1);
    assert!(parsed
        .errors
        .iter()
        .any(|error| error.code.scope.as_deref() == Some("TS")
            && error.code.number.as_deref() == Some("1029")));
    assert!(parity::script_parse_errors(source, true).is_empty());
    // The same sequence is not JavaScript, and invalid TS stays a parse gap.
    assert!(!parity::script_parse_errors(source, false).is_empty());
    for invalid in [
        "class Box { readonly protected value: number; broken = ; }",
        "class Box { readonly protected value: number;",
    ] {
        let errors = parity::script_parse_errors(invalid, true);
        assert!(!errors.is_empty(), "{invalid}");
        assert!(errors.iter().all(|error| !error.contains("must precede")));
    }
    assert!(
        !parity::script_parse_errors("class Box { readonly readonly value: number; }", true)
            .is_empty()
    );
}

#[test]
fn fixture_messages_normalize_only_the_evaluated_filename() {
    let original = "tests/fixtures/rules/example/input.svelte";
    for (absolute, cwd) in [
        (
            "/work/repo/corpus/tests/fixtures/rules/example/input.svelte",
            "/work/repo",
        ),
        (
            r"C:\work\repo\corpus\tests\fixtures\rules\example\input.svelte",
            r"C:\work\repo",
        ),
    ] {
        let portable = absolute.replace('\\', "/");
        let relative = "corpus/tests/fixtures/rules/example/input.svelte";
        for filename in [
            absolute.to_string(),
            portable.clone(),
            relative.to_string(),
            relative.replace('/', "\\"),
        ] {
            let message = format!("Error: \"{filename}:4:11: Unknown word red\"");
            assert_eq!(
                parity::fixture_message(&message, absolute, cwd, original),
                format!("Error: \"{original}:4:11: Unknown word red\"")
            );
            for other in [
                format!("{filename}.bak"),
                format!("prefix/{filename}"),
                filename.replace("input.svelte", "other.svelte"),
            ] {
                let message = format!("Error: \"{other}:4:11: Unknown word red\"");
                assert_eq!(
                    parity::fixture_message(&message, absolute, cwd, original),
                    message
                );
            }
        }
    }
    let message = "Unrelated path corpus/tests/fixtures/rules/example/input.svelte:4:11";
    assert_eq!(
        parity::fixture_message(
            message,
            "/work/repo/corpus/tests/fixtures/rules/example/input.svelte",
            "/work/other",
            original
        ),
        message
    );
}

#[test]
fn locations_use_utf16_and_javascript_line_breaks() {
    let source = "a😀b\r\nç\rX\u{2028}Y\u{2029}Z";
    assert_eq!(
        parity::location(source, "a😀".len() as u32).unwrap(),
        (1, 4)
    );
    for (needle, line) in [("ç", 2), ("X", 3), ("Y", 4), ("Z", 5)] {
        assert_eq!(
            parity::location(source, source.find(needle).unwrap() as u32).unwrap(),
            (line, 1)
        );
    }
    assert!(parity::location(source, 2).is_err());
    assert!(parity::location(source, 100).is_err());
}

#[test]
fn diagnostics_preserve_multiple_properties_at_the_same_location() {
    let diags = ["foo", "bar"].map(|message| LintDiagnostic {
        rule_name: "svelte/no-unused-props",
        message: message.into(),
        span: Span::new(0, 1),
        fix: None,
        suggestions: Vec::new(),
    });
    assert_eq!(
        parity::diagnostics("x", &diags).unwrap(),
        vec![
            json!({"message":"foo", "line":1, "column":1}),
            json!({"message":"bar", "line":1, "column":1}),
        ]
    );
}

#[test]
fn fixes_sort_and_skip_conflicts_in_a_single_pass() {
    let fixes = vec![
        Fix {
            span: Span::new(4, 5),
            replacement: "E".into(),
        },
        Fix {
            span: Span::new(1, 3),
            replacement: "BC".into(),
        },
        Fix {
            span: Span::new(2, 4),
            replacement: "overlap".into(),
        },
        Fix {
            span: Span::new(3, 4),
            replacement: "touching".into(),
        },
    ];
    assert_eq!(parity::apply_fixes("abcdef", &fixes).unwrap(), "aBCdEf");
    assert_eq!(parity::apply_fixes("unchanged", &[]).unwrap(), "unchanged");
    assert!(parity::apply_fixes(
        "😀",
        &[Fix {
            span: Span::new(1, 2),
            replacement: "bad".into()
        }]
    )
    .is_err());
}

#[test]
fn baseline_rejects_new_and_changed_gaps_but_allows_improvements() {
    let baseline = serde_json::from_value(json!({
        "known": {"diagnostics": "old", "version_skip": "gate"}
    }))
    .unwrap();
    assert!(!parity::is_regression(
        &baseline,
        "known",
        "diagnostics",
        "old",
        false
    ));
    assert!(parity::is_regression(
        &baseline,
        "known",
        "diagnostics",
        "changed",
        false
    ));
    assert!(parity::is_regression(
        &baseline,
        "new",
        "diagnostics",
        "old",
        false
    ));
    assert!(parity::is_regression(
        &baseline,
        "known",
        "fix_output",
        "old",
        false
    ));
    assert!(parity::is_regression(
        &baseline,
        "known",
        "diagnostics",
        "old",
        true
    ));
    assert!(!parity::is_regression(
        &baseline,
        "known",
        "version_skip",
        "gate",
        true
    ));
}

fn alternative(description: &str, start: u32, end: u32, replacement: &str) -> Suggestion {
    Suggestion {
        description: description.into(),
        fix: Fix {
            span: Span::new(start, end),
            replacement: replacement.into(),
        },
    }
}

#[test]
fn suggestions_keep_diagnostic_association_and_apply_alternatives_independently() {
    let source = "abc";
    let diags = vec![
        LintDiagnostic {
            rule_name: "test",
            message: "later".into(),
            span: Span::new(2, 3),
            fix: Some(Fix {
                span: Span::new(0, 3),
                replacement: "automatic".into(),
            }),
            suggestions: vec![
                alternative("first", 0, 2, "X"),
                alternative("second", 1, 3, "Y"),
            ],
        },
        LintDiagnostic {
            rule_name: "test",
            message: "earlier".into(),
            span: Span::new(0, 1),
            fix: None,
            suggestions: vec![],
        },
        LintDiagnostic {
            rule_name: "test",
            message: "same location".into(),
            span: Span::new(2, 3),
            fix: None,
            suggestions: vec![alternative("third", 0, 3, "Z")],
        },
    ];
    assert_eq!(
        parity::suggestions(source, &diags).unwrap(),
        vec![
            json!([]),
            json!([{"desc":"first", "output":"Xc"}, {"desc":"second", "output":"aY"}]),
            json!([{"desc":"third", "output":"Z"}])
        ]
    );
    assert_eq!(
        parity::apply_fixes(
            source,
            &diags
                .iter()
                .filter_map(|d| d.fix.clone())
                .collect::<Vec<_>>()
        )
        .unwrap(),
        "automatic"
    );
}

#[test]
fn suggestion_spans_validate_utf8_boundaries_and_bounds() {
    let mut diag = LintDiagnostic {
        rule_name: "test",
        message: "test".into(),
        span: Span::new(0, 4),
        fix: None,
        suggestions: vec![alternative("bad", 1, 2, "x")],
    };
    assert!(parity::suggestions("😀", &[diag.clone()]).is_err());
    diag.suggestions[0].fix.span = Span::new(0, 5);
    assert!(parity::suggestions("😀", &[diag.clone()]).is_err());
    diag.suggestions[0].fix.span = Span::new(4, 0);
    assert!(parity::suggestions("😀", &[diag]).is_err());
}

struct SuggestingRule;
impl Rule for SuggestingRule {
    fn name(&self) -> &'static str {
        "svelte/test-suggestions"
    }
    fn run(&self, ctx: &mut LintContext) {
        let start = ctx.source.find("<p>").unwrap() as u32;
        ctx.diagnostic_with_suggestions(
            "optional",
            Span::new(start, start + 3),
            vec![alternative("replace", start, start + 3, "<div>")],
        );
    }
}

#[test]
fn suggestion_only_diagnostics_follow_ignore_filtering_without_becoming_fixes() {
    let mut linter = Linter::all();
    let disabled = json!({"rules": linter.rules().iter()
        .map(|rule| (rule.name().to_string(), json!("off")))
        .collect::<serde_json::Map<_, _>>()});
    linter.remove_disabled_rules(
        &oxvelte::config::OxvelteConfig::parse(&disabled.to_string()).unwrap(),
    );
    let linter = linter.with_custom_rules(vec![Box::new(SuggestingRule)]);
    for (source, expected) in [
        ("<p>x</p>", 1),
        (
            "<!-- eslint-disable-next-line svelte/test-suggestions -->\n<p>x</p>",
            0,
        ),
    ] {
        let allocator = oxc::allocator::Allocator::default();
        let result = oxvelte::parser::parse(source, &allocator);
        let diags = linter.lint(&result.ast, source);
        assert_eq!(diags.len(), expected);
        if let Some(diag) = diags.first() {
            assert!(diag.fix.is_none());
            assert_eq!(diag.suggestions[0].description, "replace");
            assert_eq!(diag.rule_name, "svelte/test-suggestions");
        }
    }
}
