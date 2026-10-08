#[path = "support/parity.rs"]
#[allow(dead_code)]
mod parity;

use oxc::span::Span;
use oxvelte::linter::{Fix, LintDiagnostic};
use serde_json::json;

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
