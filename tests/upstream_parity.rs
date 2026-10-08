//! Run with cargo test --test upstream_parity -- [--rule NAME] [--report PATH].
//! --strict rejects every eligible gap; --update-baseline explicitly records current gaps.
#[path = "support/parity.rs"]
mod parity;

use oxc::{allocator::Allocator, parser::Parser, span::SourceType};
use oxvelte::{
    config::OxvelteConfig,
    linter::{LintDiagnostic, Linter, RuleConfig},
    parser,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const REVISION: &str = "18339c886320151148568063c5801bf69cb51027";
const CORPUS: &str = "fixtures/upstream/eslint-plugin-svelte";
const BASELINE: &str = "tests/upstream-parity-baseline.json";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema: u32,
    revision: String,
    environment: Value,
    rule_count: usize,
    files: BTreeMap<String, String>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    id: String,
    rule: String,
    kind: String,
    filename: String,
    config: Value,
    config_file: Option<String>,
    executable_config: bool,
    ineligible: Vec<String>,
    fixable: bool,
    errors: Vec<Value>,
    output: Option<String>,
}

#[derive(Default)]
struct Args {
    rule: Option<String>,
    report: Option<PathBuf>,
    update: bool,
    strict: bool,
}

type Issues = BTreeMap<String, Value>;
use parity::Signatures;

fn main() {
    if let Err(error) = run() {
        eprintln!("Upstream parity failed: {error}");
        std::process::exit(1);
    }
}

fn arguments() -> Result<Args, String> {
    let mut result = Args::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rule" => {
                result.rule = Some(
                    args.next()
                        .ok_or("--rule needs a rule name")?
                        .trim_start_matches("svelte/")
                        .into(),
                )
            }
            "--report" => result.report = Some(args.next().ok_or("--report needs a path")?.into()),
            "--update-baseline" => result.update = true,
            "--strict" => result.strict = true,
            // Cargo test can pass this standard flag when requesting visible output.
            "--nocapture" => {}
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    if result.update && (result.rule.is_some() || result.strict) {
        return Err("Baseline updates require the entire suite without --strict".into());
    }
    Ok(result)
}

fn checked_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(name);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(format!("Unsafe corpus path: {name}"));
    }
    Ok(root.join(relative))
}

fn verify_files(root: &Path, manifest: &Manifest) -> Result<(), String> {
    if manifest.schema != 1 || manifest.revision != REVISION || manifest.cases.is_empty() {
        return Err("Invalid, empty or unpinned manifest".into());
    }
    for (name, expected) in &manifest.files {
        let bytes = fs::read(checked_path(root, name)?)
            .map_err(|e| format!("Missing corpus file {name}: {e}"))?;
        if parity::hash(&bytes) != *expected {
            return Err(format!("Corpus hash differs: {name}. Restore the pinned blob; do not change upstream expectations."));
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut rules = std::collections::BTreeSet::new();
    for case in &manifest.cases {
        if !ids.insert(&case.id) || !["valid", "invalid"].contains(&case.kind.as_str()) {
            return Err(format!("Invalid or duplicate case: {}", case.id));
        }
        rules.insert(&case.rule);
        if !manifest.files.contains_key(&case.filename) {
            return Err(format!("Unhashed case input: {}", case.id));
        }
    }
    if rules.len() != manifest.rule_count {
        return Err("Manifest rule inventory differs".into());
    }
    fn inventory(root: &Path, dir: &Path, manifest: &Manifest) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("Symlinks are not allowed in the pinned corpus".into());
            }
            if kind.is_dir() {
                inventory(root, &entry.path(), manifest)?;
            } else {
                let name = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if name != "manifest.json" && !manifest.files.contains_key(&name) {
                    return Err(format!("Unexpected corpus file: {name}"));
                }
            }
        }
        Ok(())
    }
    inventory(root, root, manifest)?;
    Ok(())
}

fn isolated_linter(rule_name: &str) -> Linter {
    let mut lint = Linter::all();
    let disabled = json!({"rules": lint.rules().iter().filter(|rule| rule.name() != rule_name)
        .map(|rule| (rule.name().to_string(), json!("off"))).collect::<serde_json::Map<_, _>>()});
    lint.remove_disabled_rules(&OxvelteConfig::parse(&disabled.to_string()).unwrap());
    lint
}

fn rule_config(case: &Case, issues: &mut Issues) -> RuleConfig {
    let mut settings = case.config.get("settings").cloned();
    if let Some(language) = case.config.get("languageOptions") {
        if let Some(parser_options) = language.get("parserOptions") {
            if let Some(svelte_config) = parser_options.get("svelteConfig") {
                // These two forms specify the same route context. Keep the
                // original test filename and relative tests/fixtures path.
                if let Some(kit) = svelte_config.get("kit") {
                    let effective = settings.get_or_insert_with(|| json!({}));
                    if effective.get("svelte").is_none() {
                        effective["svelte"] = json!({});
                    }
                    effective["svelte"]["kit"] = kit.clone();
                }
                if svelte_config
                    .as_object()
                    .is_none_or(|o| o.keys().any(|key| key != "kit"))
                {
                    issues.insert("parser_configuration".into(), svelte_config.clone());
                }
            }
            if parser_options
                .as_object()
                .is_none_or(|o| o.keys().any(|key| key != "svelteConfig"))
            {
                issues.insert("parser_configuration".into(), parser_options.clone());
            }
        }
        if language
            .as_object()
            .is_none_or(|o| o.keys().any(|key| key != "parserOptions"))
        {
            issues.insert("language_configuration".into(), language.clone());
        }
    }
    RuleConfig {
        options: case.config.get("options").cloned(),
        settings,
    }
}

fn script_errors(source: &str, is_ts: bool) -> Vec<String> {
    let alloc = Allocator::default();
    let result = Parser::new(
        &alloc,
        source,
        if is_ts {
            SourceType::ts()
        } else {
            SourceType::mjs()
        },
    )
    .parse();
    result
        .errors
        .iter()
        .map(|error| error.to_string())
        .collect()
}

fn lint_case(
    lint: &Linter,
    source: &str,
    path: &str,
    config: RuleConfig,
    issues: &mut Issues,
) -> Vec<LintDiagnostic> {
    if path.ends_with(".svelte") {
        let alloc = Allocator::default();
        let result = parser::parse(source, &alloc);
        let mut errors: Vec<_> = result
            .errors
            .iter()
            .map(|error| error.to_string())
            .collect();
        for script in [&result.ast.instance, &result.ast.module]
            .into_iter()
            .flatten()
        {
            errors.extend(script_errors(
                &script.content,
                matches!(script.lang.as_deref(), Some("ts" | "typescript")),
            ));
        }
        if !errors.is_empty() {
            issues.insert("parse_errors".into(), json!(errors));
        }
        lint.lint_with_config_and_path(&result.ast, source, config, path)
    } else {
        let errors = script_errors(source, path.ends_with(".ts"));
        if !errors.is_empty() {
            issues.insert("parse_errors".into(), json!(errors));
        }
        if path.ends_with(".svelte.js") || path.ends_with(".svelte.ts") {
            lint.lint_svelte_script_with_config_and_path(
                source,
                path.ends_with(".ts"),
                config,
                path,
            )
        } else {
            lint.lint_script_with_config_and_path(source, config, path)
        }
    }
}

fn evaluate(root: &Path, case: &Case) -> Result<Issues, String> {
    let mut issues = Issues::new();
    if !case.ineligible.is_empty() {
        issues.insert("version_skip".into(), json!(case.ineligible));
        return Ok(issues);
    }
    if case.executable_config {
        issues.insert("executable_configuration".into(), json!(case.config_file));
        return Ok(issues);
    }
    let target = if case.rule.starts_with('@') {
        case.rule.clone()
    } else {
        format!("svelte/{}", case.rule)
    };
    let lint = isolated_linter(&target);
    if lint.rules().is_empty() {
        issues.insert("unsupported_rule".into(), json!(target));
        return Ok(issues);
    }
    if ["valid-compile", "no-unused-svelte-ignore"].contains(&case.rule.as_str()) {
        issues.insert(
            "compiler_capability".into(),
            json!("Upstream expectations depend on Svelte compiler warnings"),
        );
    }
    let config = rule_config(case, &mut issues);
    let source_path = checked_path(root, &case.filename)?;
    let source = fs::read_to_string(&source_path).map_err(|e| e.to_string())?;
    // Forward slashes give the same route/path behavior on Windows and Linux.
    let filename = source_path.to_string_lossy().replace('\\', "/");
    let diags = lint_case(&lint, &source, &filename, config, &mut issues);
    let expected: Vec<_> = case
        .errors
        .iter()
        .map(|d| json!({"message": d["message"], "line": d["line"], "column": d["column"]}))
        .collect();
    match parity::diagnostics(&source, &diags) {
        Ok(actual) => {
            if expected.len() != actual.len() {
                issues.insert(
                    "diagnostic_count".into(),
                    parity::mismatch(&json!(expected.len()), &json!(actual.len())),
                );
            }
            if expected != actual {
                issues.insert(
                    "diagnostics".into(),
                    parity::mismatch(&json!(expected), &json!(actual)),
                );
            }
        }
        Err(error) => {
            issues.insert("diagnostic_spans".into(), json!(error));
        }
    }
    let fixes: Vec<_> = diags.iter().filter_map(|diag| diag.fix.clone()).collect();
    if case.kind == "invalid" && case.fixable {
        match parity::apply_fixes(&source, &fixes) {
            Ok(output) => {
                let expected_output = case.output.as_deref().unwrap_or(&source);
                if expected_output != output {
                    issues.insert(
                        "fix_output".into(),
                        parity::mismatch(&json!(expected_output), &json!(output)),
                    );
                }
                if case.output.is_none() && !fixes.is_empty() {
                    issues.insert(
                        "unexpected_fix".into(),
                        json!("Expected output:null, but Oxvelte offered a fix"),
                    );
                }
            }
            Err(error) => {
                issues.insert("fix_spans".into(), json!(error));
            }
        }
    } else if !case.fixable && !fixes.is_empty() {
        issues.insert(
            "unexpected_fix".into(),
            json!("Upstream rule is not fixable"),
        );
    }
    let expected_suggestions: Vec<_> = case
        .errors
        .iter()
        .map(|d| {
            json!(d
                .get("suggestions")
                .and_then(Value::as_array)
                .map(|suggestions| suggestions
                    .iter()
                    .map(|s| json!({"desc": s["desc"], "output": s["output"]}))
                    .collect::<Vec<_>>())
                .unwrap_or_default())
        })
        .collect();
    match parity::suggestions(&source, &diags) {
        Ok(actual) => {
            // Diagnostic mismatches already describe different lengths when neither
            // side offers suggestions. Still reject unexpected alternatives.
            let has_suggestions = expected_suggestions
                .iter()
                .chain(&actual)
                .any(|s| s.as_array().is_some_and(|a| !a.is_empty()));
            if has_suggestions && expected_suggestions != actual {
                issues.insert(
                    "suggestions".into(),
                    parity::mismatch(&json!(expected_suggestions), &json!(actual)),
                );
            }
        }
        Err(error) => {
            issues.insert("suggestion_spans".into(), json!(error));
        }
    }
    Ok(issues)
}

fn run() -> Result<(), String> {
    let args = arguments()?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = repo.join(CORPUS);
    let manifest_bytes = fs::read(root.join("manifest.json")).map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|e| e.to_string())?;
    verify_files(&root, &manifest)?;
    let manifest_hash = parity::hash(&manifest_bytes);
    let mut baseline: Signatures = BTreeMap::new();
    if !args.update {
        let saved: Value = serde_json::from_slice(
            &fs::read(repo.join(BASELINE)).map_err(|e| format!("Cannot read baseline: {e}"))?,
        )
        .map_err(|e| e.to_string())?;
        if saved["revision"] != REVISION || saved["manifestHash"] != manifest_hash {
            return Err("Baseline belongs to another corpus/environment. Review the import before updating it.".into());
        }
        baseline = serde_json::from_value(saved["cases"].clone()).map_err(|e| e.to_string())?;
    }
    let selected: Vec<_> = manifest
        .cases
        .iter()
        .filter(|case| args.rule.as_ref().is_none_or(|rule| case.rule == *rule))
        .collect();
    if selected.is_empty() {
        return Err("No cases selected; check the rule name".into());
    }
    let mut signatures = Signatures::new();
    let mut results = Vec::new();
    let mut summary: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    let mut regressions = Vec::new();
    for case in &selected {
        let issues = evaluate(&root, case)?;
        let stats = summary.entry(case.rule.clone()).or_default();
        let status = if issues.is_empty() {
            stats[0] += 1;
            "pass"
        } else if issues.contains_key("version_skip") {
            stats[2] += 1;
            "version_skip"
        } else {
            stats[1] += 1;
            "gap"
        };
        for (dimension, detail) in &issues {
            let signature = parity::hash(serde_json::to_string(detail).unwrap().as_bytes());
            if !args.update
                && parity::is_regression(&baseline, &case.id, dimension, &signature, args.strict)
            {
                regressions.push(format!("{}: {dimension}", case.id));
            }
            signatures
                .entry(case.id.clone())
                .or_default()
                .insert(dimension.clone(), signature);
        }
        results.push(json!({"id": case.id, "rule": case.rule, "status": status, "issues": issues}));
    }
    println!(
        "Pinned eslint-plugin-svelte {REVISION}; {} cases selected",
        selected.len()
    );
    println!("{:<48} {:>6} {:>6} {:>6}", "Rule", "pass", "gaps", "skip");
    let mut totals = [0; 3];
    for (rule, stats) in &summary {
        println!("{rule:<48} {:>6} {:>6} {:>6}", stats[0], stats[1], stats[2]);
        for index in 0..3 {
            totals[index] += stats[index];
        }
    }
    println!(
        "{:<48} {:>6} {:>6} {:>6}",
        "TOTAL", totals[0], totals[1], totals[2]
    );
    if let Some(path) = &args.report {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(path, serde_json::to_string_pretty(&json!({"revision": REVISION, "manifestHash": manifest_hash,
            "environment": manifest.environment, "summary": summary, "totals": {"pass":totals[0], "gaps":totals[1], "versionSkip":totals[2]},
            "regressions": regressions, "cases": results})).unwrap() + "\n").map_err(|e| e.to_string())?;
        println!("Report: {}", path.display());
    }
    if args.update {
        let saved = json!({"schema":1, "revision": REVISION, "manifestHash": manifest_hash,
            "note":"Known gaps, not rewritten upstream expectations. Regenerate only after reviewing a full --report.",
            "cases": signatures});
        fs::write(
            repo.join(BASELINE),
            serde_json::to_string_pretty(&saved).unwrap() + "\n",
        )
        .map_err(|e| e.to_string())?;
        println!("Updated {BASELINE}");
    } else if !regressions.is_empty() {
        return Err(format!("{} new/changed gaps:\n{}\nUse --report for expected/actual details. Baseline updates require review.", regressions.len(), regressions.iter().take(30).cloned().collect::<Vec<_>>().join("\n")));
    } else {
        println!("No new gaps relative to the baseline. Existing gaps are listed above.");
    }
    Ok(())
}
