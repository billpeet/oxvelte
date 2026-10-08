use oxvelte::linter::{Fix, LintDiagnostic};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub type Signatures = BTreeMap<String, BTreeMap<String, String>>;

pub fn is_regression(
    baseline: &Signatures,
    id: &str,
    dimension: &str,
    signature: &str,
    strict: bool,
) -> bool {
    (strict && dimension != "version_skip")
        || baseline
            .get(id)
            .and_then(|known| known.get(dimension))
            .map(String::as_str)
            != Some(signature)
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// ESLint counts columns in UTF-16 code units and recognizes all JS line breaks.
pub fn location(source: &str, offset: u32) -> Result<(usize, usize), String> {
    let offset = offset as usize;
    if !source.is_char_boundary(offset) {
        return Err(format!("Invalid byte offset {offset}"));
    }
    let mut line = 1;
    let mut column = 1;
    let mut chars = source[..offset].chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                line += 1;
                column = 1;
            }
            '\n' | '\u{2028}' | '\u{2029}' => {
                line += 1;
                column = 1;
            }
            _ => column += ch.len_utf16(),
        }
    }
    Ok((line, column))
}

pub fn diagnostics(source: &str, diags: &[LintDiagnostic]) -> Result<Vec<Value>, String> {
    let mut located = Vec::new();
    for diag in diags {
        let (line, column) = location(source, diag.span.start)?;
        // Validate end offsets too, even though the upstream YAML omits them.
        location(source, diag.span.end)?;
        if diag.span.start > diag.span.end {
            return Err("Reversed diagnostic span".into());
        }
        located.push(json!({ "message": diag.message, "line": line, "column": column }));
    }
    // ESLint sorts findings by location, stably. Preserve duplicates and order
    // at the same location; a set comparison hides multiple unused properties.
    located.sort_by_key(|d| (d["line"].as_u64(), d["column"].as_u64()));
    Ok(located)
}

/// Keep one suggestions array per diagnostic, including empty arrays, so that
/// alternatives cannot accidentally migrate to another finding at the same location.
pub fn suggestions(source: &str, diags: &[LintDiagnostic]) -> Result<Vec<Value>, String> {
    let mut ordered = diags
        .iter()
        .map(|diag| Ok((location(source, diag.span.start)?, diag)))
        .collect::<Result<Vec<_>, String>>()?;
    // Use the same stable location ordering as diagnostics().
    ordered.sort_by_key(|(location, _)| *location);
    ordered
        .into_iter()
        .map(|(_, diag)| {
            diag.suggestions
                .iter()
                .map(|suggestion| {
                    let output = apply_fixes(source, std::slice::from_ref(&suggestion.fix))?;
                    Ok(json!({"desc": suggestion.description, "output": output}))
                })
                .collect::<Result<Vec<_>, String>>()
                .map(|suggestions| json!(suggestions))
        })
        .collect()
}

/// Apply one pass, matching the upstream fixture generator's overlap policy.
pub fn apply_fixes(source: &str, fixes: &[Fix]) -> Result<String, String> {
    let mut ordered: Vec<_> = fixes.iter().collect();
    ordered.sort_by_key(|fix| (fix.span.start, fix.span.end));
    let mut output = String::new();
    let mut last_end = None;
    for fix in ordered {
        let start = fix.span.start as usize;
        let end = fix.span.end as usize;
        if start > end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            return Err(format!("Invalid fix span {start}..{end}"));
        }
        // ESLint rejects touching edits as well as overlapping edits.
        if last_end.is_some_and(|last| last >= start) {
            continue;
        }
        output.push_str(&source[last_end.unwrap_or(0)..start]);
        output.push_str(&fix.replacement);
        last_end = Some(end);
    }
    output.push_str(&source[last_end.unwrap_or(0)..]);
    Ok(output)
}

pub fn mismatch(expected: &Value, actual: &Value) -> Value {
    json!({ "expected": expected, "actual": actual })
}
