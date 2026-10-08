//! Shared bridge to the project's Svelte compiler. The process starts only when
//! a compiler-backed rule runs and is reused across components.
use oxc::span::Span;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone)]
pub struct ReportWarning {
    pub message: String,
    pub code: Option<String>,
    pub span: Option<Span>,
}
#[derive(Debug, Clone)]
pub struct Warning {
    pub message: String,
    pub code: Option<String>,
    pub span: Option<Span>,
    pub filtered: bool,
    pub report: Option<ReportWarning>,
}
#[derive(Debug, Clone)]
pub struct IgnoreItem {
    pub code: Option<String>,
    pub code_for_v5: Option<String>,
    pub span: Span,
    pub token_span: Span,
}
#[derive(Debug, Clone)]
pub struct CompileResult {
    pub compiler_version: String,
    pub svelte_major: u32,
    pub kind: String,
    pub warnings: Vec<Warning>,
    pub unused_ignores: Vec<IgnoreItem>,
    pub ignore_items: Vec<IgnoreItem>,
    pub strip_style_elements: Vec<Span>,
}
#[derive(Deserialize)]
struct WireReport {
    message: String,
    code: Option<String>,
    start: Option<usize>,
    end: Option<usize>,
}
#[derive(Deserialize)]
struct WireWarning {
    #[serde(flatten)]
    warning: WireReport,
    filtered: bool,
    report: Option<WireReport>,
}
#[derive(Deserialize)]
struct WireResult {
    compiler_version: String,
    svelte_major: u32,
    kind: String,
    warnings: Vec<WireWarning>,
    strip_style_elements: Vec<[u32; 2]>,
}
#[derive(Deserialize)]
struct Response {
    result: Option<WireResult>,
    error: Option<String>,
}
#[derive(Serialize)]
struct Request<'a> {
    source: &'a str,
    filename: Option<&'a str>,
    settings: Option<&'a Value>,
}
struct Runtime {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    cache: HashMap<String, CompileResult>,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Runtime {
    fn start() -> Result<Self, String> {
        let mut child = Command::new(std::env::var_os("OXVELTE_NODE").unwrap_or_else(|| "node".into()))
            .args(["--input-type=commonjs", "-e", include_str!("../scripts/compiler-runtime/bridge.cjs")])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
            .spawn().map_err(|e| format!("Cannot start Node.js for Svelte compiler rules: {e}. Install Node.js or set OXVELTE_NODE."))?;
        let input = child
            .stdin
            .take()
            .ok_or("Compiler runtime has no input pipe")?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or("Compiler runtime has no output pipe")?,
        );
        Ok(Self {
            child,
            input,
            output,
            cache: HashMap::new(),
        })
    }
    fn compile(
        &mut self,
        source: &str,
        filename: Option<&str>,
        settings: Option<&Value>,
    ) -> Result<CompileResult, String> {
        let key = serde_json::to_string(&Request {
            source,
            filename,
            settings,
        })
        .map_err(|e| e.to_string())?;
        if let Some(result) = self.cache.get(&key) {
            return Ok(result.clone());
        }
        self.input
            .write_all(key.as_bytes())
            .and_then(|_| self.input.write_all(b"\n"))
            .and_then(|_| self.input.flush())
            .map_err(|e| format!("Cannot write compiler request: {e}"))?;
        let mut line = String::new();
        self.output
            .read_line(&mut line)
            .map_err(|e| format!("Cannot read compiler response: {e}"))?;
        if line.is_empty() {
            return Err("Svelte compiler runtime exited without a response".into());
        }
        let response: Response = serde_json::from_str(&line)
            .map_err(|e| format!("Invalid Svelte compiler response: {e}"))?;
        if let Some(error) = response.error {
            return Err(error);
        }
        let wire = response
            .result
            .ok_or("Compiler response did not contain a result")?;
        let report = |w: WireReport| ReportWarning {
            message: w.message,
            code: w.code,
            span: w.start.or(w.end).map(|start| {
                Span::new(
                    utf16_byte(source, start),
                    utf16_byte(source, w.end.unwrap_or(start)),
                )
            }),
        };
        let result = CompileResult {
            compiler_version: wire.compiler_version,
            svelte_major: wire.svelte_major,
            kind: wire.kind,
            warnings: wire
                .warnings
                .into_iter()
                .map(|w| {
                    let raw = report(w.warning);
                    Warning {
                        message: raw.message,
                        code: raw.code,
                        span: raw.span,
                        filtered: w.filtered,
                        report: w.report.map(report),
                    }
                })
                .collect(),
            unused_ignores: vec![],
            ignore_items: vec![],
            strip_style_elements: wire
                .strip_style_elements
                .into_iter()
                .map(|[start, end]| Span::new(start, end))
                .collect(),
        };
        // Keep memory bounded in long-running editor integrations.
        if self.cache.len() >= 256 {
            self.cache.clear();
        }
        self.cache.insert(key, result.clone());
        Ok(result)
    }
}
fn utf16_byte(source: &str, index: usize) -> u32 {
    let mut units = 0;
    for (byte, ch) in source.char_indices() {
        if units >= index {
            return byte as u32;
        }
        units += ch.len_utf16();
    }
    source.len() as u32
}
/// Compile using Svelte resolved relative to the component, or the explicit
/// OXVELTE_COMPILER_RUNTIME directory used by reproducible parity runs.
pub fn compile(
    source: &str,
    filename: Option<&str>,
    settings: Option<&Value>,
) -> Result<CompileResult, String> {
    static RUNTIME: OnceLock<Mutex<Option<Runtime>>> = OnceLock::new();
    let mut state = RUNTIME
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "Svelte compiler runtime lock is poisoned")?;
    if state.is_none() {
        *state = Some(Runtime::start()?);
    }
    let result = state.as_mut().unwrap().compile(source, filename, settings);
    if result.as_ref().is_err_and(|message| {
        message.contains("compiler runtime exited")
            || message.contains("Cannot write compiler request")
    }) {
        *state = None;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf16_positions_map_to_utf8_boundaries() {
        assert_eq!(utf16_byte("a🦀é", 1), 1);
        assert_eq!(utf16_byte("a🦀é", 3), 5);
        assert_eq!(utf16_byte("a🦀é", 4), 7);
    }
}
