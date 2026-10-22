use oxvelte::{linter, parser};

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "oxvelte",
    version,
    about = "A fast Svelte linter powered by oxc"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the linter on Svelte files.
    Lint {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Path to an oxvelte.config.json file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Run all rules instead of just recommended ones.
        #[arg(long)]
        all_rules: bool,
        /// Auto-fix problems (where supported).
        #[arg(long)]
        fix: bool,
        /// Output results as JSON.
        #[arg(long)]
        json: bool,
        /// Suppress warnings, only show errors.
        #[arg(long, short)]
        quiet: bool,
    },
    /// Parse a Svelte file and dump the AST as JSON.
    Parse {
        file: PathBuf,
        /// Pretty-print the JSON output.
        #[arg(long, short)]
        pretty: bool,
        /// Output format: "legacy" (Svelte 4) or "modern" (Svelte 5).
        #[arg(long, default_value = "legacy")]
        format: String,
    },
    /// Parse + lint (alias for lint).
    Check {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// List all available lint rules.
    Rules,
    /// Migrate an ESLint svelte config to oxvelte.config.json.
    Migrate {
        /// Path to ESLint config file (.eslintrc.json, eslint.config.json, etc.)
        file: PathBuf,
        /// Write output to oxvelte.config.json instead of stdout.
        #[arg(long, short)]
        write: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Lint {
            paths,
            config,
            all_rules,
            fix,
            json,
            quiet,
        } => cmd_lint(&paths, config.as_ref(), all_rules, fix, json, quiet),
        Command::Parse {
            file,
            pretty,
            format,
        } => cmd_parse(&file, pretty, &format),
        Command::Check { paths } => cmd_lint(&paths, None, false, false, false, false),
        Command::Rules => cmd_rules(),
        Command::Migrate { file, write } => cmd_convert_config(&file, write),
    }
}

struct FileDiagnostic {
    line: usize,
    col: usize,
    end_line: usize,
    end_col: usize,
    message: String,
    rule_name: &'static str,
    severity: DiagnosticSeverity,
    suggestions: Vec<linter::Suggestion>,
}

struct FileResult {
    path: String,
    diagnostics: Vec<FileDiagnostic>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DiagnosticSeverity {
    Warning,
    Error,
}

impl DiagnosticSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

fn load_lint_config(
    search_dir: &std::path::Path,
    config_path: Option<&PathBuf>,
) -> Result<(oxvelte::config::OxvelteConfig, PathBuf), String> {
    if let Some(path) = config_path {
        let path = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()
                .map_err(|e| format!("Error reading current directory: {e}"))?
                .join(path)
        };
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Error reading config {}: {e}", path.display()))?;
        let config = oxvelte::config::OxvelteConfig::parse(&content)
            .map_err(|e| format!("Error parsing config {}: {e}", path.display()))?;
        let config_dir = path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        return Ok((config, config_dir));
    }

    // Auto-discovery: the directory the config was found in is the base for
    // relative paths inside it (e.g. customRules globs), and an unreadable or
    // invalid discovered config is a hard error, same as --config.
    oxvelte::config::OxvelteConfig::load_with_dir(search_dir)
}

/// Maximum autofix passes over a single file, matching ESLint's cap — fixes
/// from one pass can unlock new fixable diagnostics in the next.
const MAX_FIX_PASSES: usize = 10;

/// Apply auto-fixes to `source`, ESLint-style: fixes are applied in span
/// order, and any fix that is malformed, out of bounds, not on a UTF-8
/// boundary, or overlaps an already-applied fix is skipped — whatever caused
/// it is reported again after the fix pass.
fn apply_fixes(source: &str, fixes: &[&linter::Fix]) -> String {
    let mut ordered = fixes.to_vec();
    ordered.sort_by_key(|fix| (fix.span.start, fix.span.end));
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for fix in ordered {
        let start = fix.span.start as usize;
        let end = fix.span.end as usize;
        if start < cursor || start > end || end > source.len() {
            continue;
        }
        if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            continue;
        }
        out.push_str(&source[cursor..start]);
        out.push_str(&fix.replacement);
        cursor = end;
    }
    out.push_str(&source[cursor..]);
    out
}

fn cmd_lint(
    paths: &[PathBuf],
    config_path: Option<&PathBuf>,
    all_rules: bool,
    fix: bool,
    json_output: bool,
    quiet: bool,
) -> ExitCode {
    use rayon::prelude::*;

    // Load config (walks up from the first path, or cwd).
    let config_dir = paths
        .first()
        .and_then(|p| {
            if p.is_dir() {
                Some(p.clone())
            } else {
                p.parent().map(|p| p.to_path_buf())
            }
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let (config, config_dir) = match load_lint_config(&config_dir, config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    #[cfg(not(feature = "custom-rules"))]
    let _ = &config_dir;

    #[allow(unused_mut)]
    let mut lint = if all_rules {
        linter::Linter::all()
    } else {
        linter::Linter::recommended()
    };

    // Add rules the config switches on, then load custom JS rules (when the
    // feature is enabled), and only then drop the rules the config switches
    // off — a "custom/foo": "off" entry must apply to custom rules too.
    lint.add_enabled_rules(&config);

    // Load custom JS rules when the feature is enabled.
    #[cfg(feature = "custom-rules")]
    {
        if !config.custom_rules.is_empty() {
            let custom =
                oxvelte::custom_rules::load_custom_rules(&config.custom_rules, &config_dir);
            lint = lint.with_custom_rules(custom);
        }
    }

    lint.remove_disabled_rules(&config);

    let files = collect_lint_files(paths);
    let file_count = files.len();
    let use_color = !json_output && atty::is(atty::Stream::Stderr);
    let start = std::time::Instant::now();

    let mut file_results: Vec<FileResult> = files
        .par_iter()
        .filter_map(|path| {
            let mut source = match std::fs::read_to_string(path) {
                Ok(source) => source,
                // A file that vanished between collection and linting was
                // moved, renamed, or deleted mid-run — skip it quietly.
                // Anything else (permissions, invalid UTF-8, …) means a real
                // file went unlinted, which is worth surfacing.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
                Err(e) => {
                    eprintln!("oxvelte: could not read {}: {e}", path.display());
                    return None;
                }
            };
            let path_str = path.to_string_lossy();
            let is_svelte = path.extension().is_some_and(|e| e == "svelte");
            let is_svelte_module =
                path_str.ends_with(".svelte.js") || path_str.ends_with(".svelte.ts");
            let file_path_owned = path_str.to_string();
            // None means an internal panic — the file is skipped entirely.
            let run_lint = |source: &str| -> Option<Vec<linter::LintDiagnostic>> {
                if is_svelte {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let alloc = oxc::allocator::Allocator::default();
                        let result = parser::parse_for_lint(source, &alloc);
                        // Surface template parse errors as error-severity
                        // diagnostics — a malformed file must never lint clean.
                        let mut diags: Vec<linter::LintDiagnostic> = result
                            .errors
                            .iter()
                            .map(|error| {
                                let span = error
                                    .labels
                                    .as_ref()
                                    .and_then(|labels| labels.first())
                                    .map(|label| {
                                        oxc::span::Span::new(
                                            label.offset() as u32,
                                            (label.offset() + label.len()) as u32,
                                        )
                                    })
                                    .unwrap_or_else(|| oxc::span::Span::new(0, 0));
                                linter::LintDiagnostic {
                                    rule_name: "oxvelte/parse-error",
                                    message: error.to_string(),
                                    span,
                                    fix: None,
                                    suggestions: Vec::new(),
                                }
                            })
                            .collect();
                        diags.extend(lint.lint_with_project_config_and_path(
                            &result.ast,
                            source,
                            &config,
                            &file_path_owned,
                        ));
                        diags
                    })) {
                        Ok(d) => Some(d),
                        Err(_) => {
                            eprintln!("oxvelte: internal error parsing {}", path.display());
                            None
                        }
                    }
                } else if is_svelte_module {
                    let is_ts = path_str.ends_with(".ts");
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        lint.lint_svelte_script_with_project_config_and_path(
                            source,
                            is_ts,
                            &config,
                            &file_path_owned,
                        )
                    })) {
                        Ok(d) => Some(d),
                        Err(_) => {
                            eprintln!("oxvelte: internal error linting {}", path.display());
                            None
                        }
                    }
                } else {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        lint.lint_script_with_project_config_and_path(
                            source,
                            &config,
                            &file_path_owned,
                        )
                    })) {
                        Ok(d) => Some(d),
                        Err(_) => {
                            eprintln!("oxvelte: internal error linting {}", path.display());
                            None
                        }
                    }
                }
            };
            let mut diags = run_lint(&source)?;
            if fix {
                // ESLint-style autofix: apply each rule's top-level fix in
                // span order (skipping overlaps), then re-lint, up to a pass
                // cap. Suggestions are never auto-applied, and suppressed
                // diagnostics carry no fix here, so they are left untouched.
                let original = source.clone();
                let mut changed = false;
                for _ in 0..MAX_FIX_PASSES {
                    let fixes: Vec<&linter::Fix> =
                        diags.iter().filter_map(|d| d.fix.as_ref()).collect();
                    if fixes.is_empty() {
                        break;
                    }
                    let fixed_source = apply_fixes(&source, &fixes);
                    if fixed_source == source {
                        break;
                    }
                    source = fixed_source;
                    changed = true;
                    diags = run_lint(&source)?;
                }
                if changed {
                    if let Err(e) = std::fs::write(path, &source) {
                        eprintln!(
                            "oxvelte: could not write fixes to {}: {e}",
                            path.display()
                        );
                        source = original;
                        diags = run_lint(&source)?;
                    }
                }
            }
            if diags.is_empty() {
                return None;
            }
            let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            let path_str = abs_path.display().to_string();
            let diagnostics: Vec<FileDiagnostic> = diags
                .iter()
                .map(|d| {
                    let (line, col) = offset_to_line_col(&source, d.span.start as usize);
                    let (end_line, end_col) = offset_to_line_col(&source, d.span.end as usize);
                    FileDiagnostic {
                        line,
                        col,
                        end_line,
                        end_col,
                        message: d.message.clone(),
                        rule_name: d.rule_name,
                        severity: diagnostic_severity(&config, d.rule_name),
                        suggestions: d.suggestions.clone(),
                    }
                })
                .collect();
            Some(FileResult {
                path: path_str,
                diagnostics,
            })
        })
        .collect();

    file_results.sort_by(|a, b| a.path.cmp(&b.path));

    let total_diags: usize = file_results.iter().map(|f| f.diagnostics.len()).sum();
    let files_with_diags = file_results.len();
    let error_count: usize = file_results
        .iter()
        .flat_map(|f| f.diagnostics.iter())
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .count();
    let warning_count = total_diags.saturating_sub(error_count);

    if json_output {
        let json_results: Vec<serde_json::Value> = visible_file_diagnostics(&file_results, quiet)
            .into_iter()
            .map(|(f, d)| diagnostic_json(&f.path, d))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json_results).unwrap_or_default()
        );
    } else if !quiet {
        print_diagnostics(&file_results, use_color, false);

        if total_diags > 0 {
            print_summary(
                total_diags,
                warning_count,
                error_count,
                files_with_diags,
                use_color,
            );
        }
    }
    if quiet {
        // Text mode: --quiet suppresses warnings but errors are still shown,
        // matching the CLI help text and the JSON path's filtering.
        if !json_output {
            print_diagnostics(&file_results, use_color, true);
        }
        let elapsed = start.elapsed();
        if use_color {
            if error_count > 0 {
                eprintln!("\x1b[1m{}\x1b[0m file(s) scanned in \x1b[1m{:.0?}\x1b[0m, \x1b[1;31m{} error(s)\x1b[0m and \x1b[1m{} warning(s)\x1b[0m found.", file_count, elapsed, error_count, warning_count);
            } else {
                eprintln!("\x1b[1m{}\x1b[0m file(s) scanned in \x1b[1m{:.0?}\x1b[0m, \x1b[1;32m0 errors\x1b[0m and \x1b[1m{} warning(s)\x1b[0m found.", file_count, elapsed, warning_count);
            }
        } else {
            eprintln!(
                "{} file(s) scanned in {:.0?}, {} error(s) and {} warning(s) found.",
                file_count, elapsed, error_count, warning_count
            );
        }
    }
    if error_count > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn diagnostic_severity(
    config: &oxvelte::config::OxvelteConfig,
    rule_name: &str,
) -> DiagnosticSeverity {
    // Template parse errors are always errors (like ESLint's fatal parse
    // failures); they are not configurable.
    if rule_name == "oxvelte/parse-error" {
        return DiagnosticSeverity::Error;
    }
    let Some(entry) = config.rules.get(rule_name) else {
        return DiagnosticSeverity::Warning;
    };

    if entry.severity == "error" || entry.severity == "2" {
        DiagnosticSeverity::Error
    } else {
        DiagnosticSeverity::Warning
    }
}

fn visible_file_diagnostics<'a>(
    file_results: &'a [FileResult],
    quiet: bool,
) -> Vec<(&'a FileResult, &'a FileDiagnostic)> {
    file_results
        .iter()
        .flat_map(|f| {
            f.diagnostics.iter().filter_map(move |d| {
                if quiet && d.severity == DiagnosticSeverity::Warning {
                    None
                } else {
                    Some((f, d))
                }
            })
        })
        .collect()
}

fn print_diagnostics(file_results: &[FileResult], use_color: bool, errors_only: bool) {
    for file_result in file_results {
        let visible: Vec<&FileDiagnostic> = file_result
            .diagnostics
            .iter()
            .filter(|d| !errors_only || d.severity == DiagnosticSeverity::Error)
            .collect();
        if visible.is_empty() {
            continue;
        }
        // File header — underlined, clickable in VS Code
        if use_color {
            eprintln!("\n\x1b[4m{}\x1b[0m", file_result.path);
        } else {
            eprintln!("\n{}", file_result.path);
        }

        // Find max widths for alignment
        let max_loc_width = visible
            .iter()
            .map(|d| format!("{}:{}", d.line, d.col).len())
            .max()
            .unwrap_or(0);

        for d in &visible {
            let loc = format!("{}:{}", d.line, d.col);
            if use_color {
                let color = match d.severity {
                    DiagnosticSeverity::Warning => "\x1b[33m",
                    DiagnosticSeverity::Error => "\x1b[31m",
                };
                eprintln!(
                    "  \x1b[2m{:<width$}\x1b[0m  {}{}\x1b[0m  {}  \x1b[2m{}\x1b[0m",
                    loc,
                    color,
                    d.severity.as_str(),
                    d.message,
                    d.rule_name,
                    width = max_loc_width
                );
            } else {
                eprintln!(
                    "  {:<width$}  {}  {}  {}",
                    loc,
                    d.severity.as_str(),
                    d.message,
                    d.rule_name,
                    width = max_loc_width
                );
            }
        }
    }
}

fn print_summary(
    total_diags: usize,
    warning_count: usize,
    error_count: usize,
    files_with_diags: usize,
    use_color: bool,
) {
    if use_color {
        let color = if error_count > 0 {
            "\x1b[1;31m"
        } else {
            "\x1b[1;33m"
        };
        eprintln!(
            "\n{}{} problem(s) in {} file(s): {} error(s), {} warning(s).\x1b[0m",
            color, total_diags, files_with_diags, error_count, warning_count
        );
    } else {
        eprintln!(
            "\n{} problem(s) in {} file(s): {} error(s), {} warning(s).",
            total_diags, files_with_diags, error_count, warning_count
        );
    }
}

fn cmd_parse(file: &PathBuf, pretty: bool, format: &str) -> ExitCode {
    use oxvelte::parser::serialize::{to_legacy_json, to_modern_json};

    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", file.display(), e);
            return ExitCode::from(1);
        }
    };
    let alloc = oxc::allocator::Allocator::default();
    let result = parser::parse(&source, &alloc);
    for err in &result.errors {
        eprintln!("Parse error: {:?}", err);
    }

    let json_value = match format {
        "modern" => to_modern_json(&result.ast, &source),
        _ => to_legacy_json(&result.ast, &source),
    };

    let output = if pretty {
        serde_json::to_string_pretty(&json_value)
    } else {
        serde_json::to_string(&json_value)
    };
    match output {
        Ok(j) => {
            println!("{}", j);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("JSON error: {}", e);
            ExitCode::from(1)
        }
    }
}

fn cmd_rules() -> ExitCode {
    let rules = linter::rules::all_rules();
    let recommended = linter::rules::recommended_rules();
    let rec_names: std::collections::HashSet<String> =
        recommended.iter().map(|r| r.name().to_string()).collect();

    println!("{:<50} {:>5}  {:>5}", "Rule", "Rec", "Fix");
    println!("{}", "-".repeat(65));
    for rule in &rules {
        let name = rule.name();
        let is_rec = if rec_names.contains(name) {
            "  *  "
        } else {
            "     "
        };
        let is_fix = if rule.is_fixable() { "  *  " } else { "     " };
        println!("{:<50} {:>5}  {:>5}", name, is_rec, is_fix);
    }
    println!(
        "\n{} rules total ({} recommended)",
        rules.len(),
        recommended.len()
    );
    ExitCode::SUCCESS
}

fn cmd_convert_config(file: &PathBuf, write: bool) -> ExitCode {
    let content = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", file.display(), e);
            return ExitCode::from(1);
        }
    };

    // Try to extract JSON from JS/MJS config files (common flat config pattern)
    let json_str = if file
        .extension()
        .is_some_and(|e| e == "mjs" || e == "js" || e == "cjs")
    {
        eprintln!("Note: JS config files are partially supported. Only inline JSON objects with svelte rules will be extracted.");
        eprintln!("For best results, export your ESLint config as JSON first:");
        eprintln!("  npx eslint --print-config yourfile.svelte > eslint-resolved.json");
        eprintln!("  oxvelte convert-config eslint-resolved.json");
        // Try to extract rules from JS — look for "svelte/" patterns
        extract_rules_from_js(&content)
    } else {
        content.clone()
    };

    let config = match oxvelte::config::OxvelteConfig::from_eslint(&json_str) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error parsing config: {}", e);
            return ExitCode::from(1);
        }
    };

    // Detect custom plugin rules and classify them
    let config_dir = file.parent().unwrap_or(std::path::Path::new("."));
    emit_custom_rule_hints(&content, config_dir);

    if config.rules.is_empty() {
        eprintln!("No svelte/* rules found in the config.");
        return ExitCode::from(1);
    }

    let output = config.to_json();

    if write {
        match std::fs::write("oxvelte.config.json", &output) {
            Ok(_) => {
                eprintln!("Wrote oxvelte.config.json ({} rules)", config.rules.len());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Error writing oxvelte.config.json: {}", e);
                ExitCode::from(1)
            }
        }
    } else {
        println!("{}", output);
        ExitCode::SUCCESS
    }
}

/// ESTree visitor keys that signal a JS/TS-only rule.
const JS_ONLY_VISITORS: &[&str] = &[
    "ImportDeclaration",
    "ExportNamedDeclaration",
    "ExportDefaultDeclaration",
    "ExportAllDeclaration",
    "VariableDeclaration",
    "VariableDeclarator",
    "FunctionDeclaration",
    "FunctionExpression",
    "ArrowFunctionExpression",
    "ClassDeclaration",
    "ClassExpression",
    "CallExpression",
    "MemberExpression",
    "TSSatisfiesExpression",
    "TSAsExpression",
    "TSTypeAnnotation",
    "TSTypeReference",
    "Program:exit",
];

/// Svelte-specific visitor keys used by eslint-plugin-svelte rules.
const SVELTE_VISITORS: &[&str] = &[
    "SvelteElement",
    "SvelteStartTag",
    "SvelteEndTag",
    "SvelteAttribute",
    "SvelteDirective",
    "SvelteSpecialDirective",
    "SvelteIfBlock",
    "SvelteEachBlock",
    "SvelteAwaitBlock",
    "SvelteKeyBlock",
    "SvelteText",
    "SvelteMustacheTag",
    "SvelteRawMustacheTag",
    "SvelteSnippetBlock",
    "SvelteRenderTag",
    "SvelteDebugTag",
    "SvelteConstTag",
    "SvelteScriptElement",
    "SvelteStyleElement",
    "SvelteFragment",
];

#[derive(Debug, PartialEq)]
enum RuleLayer {
    JsTsOnly,
    SvelteTemplate,
    Mixed,
    Unknown,
}

fn classify_rule_file(source: &str) -> RuleLayer {
    let has_svelte = SVELTE_VISITORS.iter().any(|v| source.contains(v));
    let has_js = JS_ONLY_VISITORS.iter().any(|v| source.contains(v));
    match (has_svelte, has_js) {
        (true, true) => RuleLayer::Mixed,
        (true, false) => RuleLayer::SvelteTemplate,
        (false, true) => RuleLayer::JsTsOnly,
        (false, false) => RuleLayer::Unknown,
    }
}

/// Scan the ESLint config source for custom rule file references and emit migration hints.
fn emit_custom_rule_hints(config_source: &str, config_dir: &std::path::Path) {
    // Find local plugin/rule file references: require('./...') or import '...' paths ending in .js/.ts/.cjs/.mjs
    let mut rule_files: Vec<String> = Vec::new();
    for line in config_source.lines() {
        for pattern in &["require('", "require(\"", "from '", "from \""] {
            if let Some(start) = line.find(pattern) {
                let rest = &line[start + pattern.len()..];
                let quote = if pattern.ends_with('\'') { '\'' } else { '"' };
                if let Some(end) = rest.find(quote) {
                    let path = &rest[..end];
                    if (path.starts_with("./") || path.starts_with("../"))
                        && path.ends_with(|c: char| matches!(c, 's' | 'j'))
                    {
                        rule_files.push(path.to_string());
                    }
                }
            }
        }
    }
    rule_files.dedup();

    for rel_path in &rule_files {
        let full_path = config_dir.join(rel_path.trim_start_matches("./"));
        let source = match std::fs::read_to_string(&full_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        match classify_rule_file(&source) {
            RuleLayer::JsTsOnly => {
                eprintln!(
                    "\nCustom rule file {} uses only JS/TS visitors — oxvelte is the wrong layer.",
                    rel_path
                );
                eprintln!(
                    "  Implement it as an oxlint JS plugin instead: https://oxc.rs/docs/guide/usage/linter/plugins"
                );
            }
            RuleLayer::Mixed => {
                eprintln!(
                    "\nCustom rule file {} uses both Svelte template and JS/TS visitors.",
                    rel_path
                );
                eprintln!(
                    "  Split it: template checks → oxvelte custom rule, script checks → oxlint plugin."
                );
            }
            RuleLayer::SvelteTemplate => {
                eprintln!(
                    "\nCustom rule file {} uses Svelte template visitors — migrate to an oxvelte custom rule.",
                    rel_path
                );
                eprintln!("  See: docs/custom-rules.md");
            }
            RuleLayer::Unknown => {}
        }
    }
}

/// Best-effort extraction of svelte rules from a JS config file.
fn extract_rules_from_js(content: &str) -> String {
    let mut rules = serde_json::Map::new();
    // Find patterns like "svelte/rule-name": "error" or 'svelte/rule-name': ['error', {...}]
    for line in content.lines() {
        let trimmed = line.trim().trim_end_matches(',');
        // Match: "svelte/rule-name": value  or  'svelte/rule-name': value
        for quote in &["\"", "'"] {
            let prefix = format!("{}svelte/", quote);
            if let Some(start) = trimmed.find(&prefix) {
                let after = &trimmed[start + 1..]; // skip opening quote
                if let Some(end) = after.find(*quote) {
                    let rule_name = &after[..end];
                    // Find the value after :
                    let rest = &after[end + 1..].trim_start();
                    if let Some(rest) = rest.strip_prefix(':') {
                        let val = rest.trim().trim_end_matches(',');
                        // Try to parse as JSON
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(val) {
                            rules.insert(
                                format!(
                                    "svelte/{}",
                                    rule_name.strip_prefix("svelte/").unwrap_or(rule_name)
                                ),
                                v,
                            );
                        } else {
                            // Try with quotes normalized
                            let normalized = val.replace('\'', "\"");
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&normalized) {
                                rules.insert(
                                    format!(
                                        "svelte/{}",
                                        rule_name.strip_prefix("svelte/").unwrap_or(rule_name)
                                    ),
                                    v,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    let obj = serde_json::json!({ "rules": rules });
    serde_json::to_string(&obj).unwrap_or_default()
}

fn diagnostic_json(path: &str, diagnostic: &FileDiagnostic) -> serde_json::Value {
    let mut value = serde_json::json!({
        "file": path,
        "rule": diagnostic.rule_name,
        "severity": diagnostic.severity.as_str(),
        "message": diagnostic.message,
        "line": diagnostic.line,
        "column": diagnostic.col,
        "endLine": diagnostic.end_line,
        "endColumn": diagnostic.end_col,
    });
    if !diagnostic.suggestions.is_empty() {
        // Replacement ranges are byte offsets into the UTF-8 source, like Fix::span.
        value["suggestions"] =
            serde_json::json!(diagnostic.suggestions.iter().map(|s| {
            serde_json::json!({
                "desc": s.description,
                "fix": {"range": [s.fix.span.start, s.fix.span.end], "text": s.fix.replacement}
            })
        }).collect::<Vec<_>>());
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_preserves_existing_fields_and_exposes_optional_alternatives() {
        let mut diagnostic = FileDiagnostic {
            line: 1,
            col: 2,
            end_line: 1,
            end_col: 3,
            message: "message".into(),
            rule_name: "svelte/test",
            severity: DiagnosticSeverity::Error,
            suggestions: Vec::new(),
        };
        let original = serde_json::json!({"file":"test.svelte", "rule":"svelte/test",
            "severity":"error", "message":"message", "line":1, "column":2,
            "endLine":1, "endColumn":3});
        assert_eq!(diagnostic_json("test.svelte", &diagnostic), original);
        diagnostic.suggestions.push(linter::Suggestion {
            description: "replace".into(),
            fix: linter::Fix {
                span: oxc::span::Span::new(1, 2),
                replacement: "text".into(),
            },
        });
        let mut expected = original;
        expected["suggestions"] = serde_json::json!([{ "desc":"replace",
            "fix":{"range":[1,2],"text":"text"}}]);
        assert_eq!(diagnostic_json("test.svelte", &diagnostic), expected);
    }

    fn fix(start: u32, end: u32, replacement: &str) -> linter::Fix {
        linter::Fix {
            span: oxc::span::Span::new(start, end),
            replacement: replacement.into(),
        }
    }

    #[test]
    fn apply_fixes_replaces_inserts_and_deletes() {
        let source = "let  a = 1;\nfoo  \n";
        let fixes = [
            fix(4, 5, ""),     // collapse double space
            fix(15, 17, ""),   // strip trailing spaces
            fix(0, 0, "// "),  // insertion at the start
        ];
        let refs: Vec<&linter::Fix> = fixes.iter().collect();
        assert_eq!(apply_fixes(source, &refs), "// let a = 1;\nfoo\n");
    }

    #[test]
    fn apply_fixes_skips_overlapping_and_keeps_first() {
        let source = "abcdef";
        let fixes = [fix(1, 4, "X"), fix(2, 3, "Y"), fix(4, 6, "Z")];
        let refs: Vec<&linter::Fix> = fixes.iter().collect();
        // The inner Y fix overlaps the already-applied X fix and is skipped;
        // Z starts exactly at X's end and still applies.
        assert_eq!(apply_fixes(source, &refs), "aXZ");
    }

    #[test]
    fn apply_fixes_skips_out_of_bounds_and_inverted_spans() {
        let source = "abc";
        let fixes = [fix(2, 99, "X"), fix(2, 1, "Y"), fix(1, 2, "Z")];
        let refs: Vec<&linter::Fix> = fixes.iter().collect();
        assert_eq!(apply_fixes(source, &refs), "aZc");
    }

    #[test]
    fn apply_fixes_skips_spans_off_utf8_boundaries() {
        let source = "aéb"; // é is two bytes: offsets 1..3
        let fixes = [fix(1, 2, "X"), fix(3, 4, "Z")];
        let refs: Vec<&linter::Fix> = fixes.iter().collect();
        // 1..2 splits the é, so only the boundary-safe fix applies.
        assert_eq!(apply_fixes(source, &refs), "aéZ");
    }

    #[test]
    fn collect_lint_files_dedupes_overlapping_inputs() {
        let dir = std::env::temp_dir().join(format!("oxvelte-dedup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("App.svelte");
        std::fs::write(&file, "<div></div>").unwrap();
        // A directory and a file inside it must not lint the file twice...
        assert_eq!(collect_lint_files(&[dir.clone(), file.clone()]).len(), 1);
        // ...and neither must two different spellings of the same file.
        let spelled = dir.join("./App.svelte");
        assert_eq!(collect_lint_files(&[file, spelled]).len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn collect_lint_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let extensions = ["svelte", "js", "ts", "mjs", "mts"];
    let mut stack: Vec<PathBuf> = paths.to_vec();
    while let Some(path) = stack.pop() {
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext) {
                    // Overlapping inputs (`lint src src/App.svelte`, or the
                    // same file spelled differently) must not lint twice.
                    let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                    if seen.insert(key) {
                        files.push(path);
                    }
                }
            }
        } else if path.is_dir() {
            let dirname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(
                dirname,
                "node_modules" | ".svelte-kit" | "build" | "dist" | ".git"
            ) {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    stack.push(entry.path());
                }
            }
        }
    }
    files
}
