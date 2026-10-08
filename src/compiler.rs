//! Shared bridge to the project's Svelte compiler. The process starts only when
//! a compiler-backed rule runs and is reused across components.
use oxc::span::Span;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
    pub metadata: Value,
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
#[derive(Deserialize, Serialize)]
struct WireReport {
    #[serde(default)]
    metadata: Value,
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
struct Response<T> {
    result: Option<T>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct WireCallbacks {
    warnings: Vec<WireWarning>,
}
#[derive(Serialize)]
struct CallbackRequest<'a> {
    operation: &'static str,
    #[serde(flatten)]
    request: Request<'a>,
    warnings: Vec<WireReport>,
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
        let script = format!(
            "const typescriptService = (() => {{ const module = {{exports: {{}}}}; {} ; return module.exports; }})();\nconst typescriptConditions = (() => {{ const module = {{exports: {{}}}}; {} ; return module.exports; }})();\n{}",
            include_str!("../scripts/compiler-runtime/typescript-service.cjs"),
            include_str!("../scripts/compiler-runtime/typescript-conditions.cjs"),
            include_str!("../scripts/compiler-runtime/bridge.cjs"),
        );
        let mut child = Command::new(std::env::var_os("OXVELTE_NODE").unwrap_or_else(|| "node".into()))
            .args(["--input-type=commonjs", "-e", "const fs=require('node:fs');const b=Buffer.alloc(1);let n='';while(fs.readSync(0,b,0,1,null)&&b[0]!==10)n+=b.toString();const code=Buffer.alloc(Number(n));let offset=0;while(offset<code.length){const count=fs.readSync(0,code,offset,code.length-offset,null);if(!count)throw Error('Incomplete Oxvelte runtime bootstrap');offset+=count;}eval(code.toString('utf8'));"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
            .spawn().map_err(|e| format!("Cannot start Node.js for Svelte compiler rules: {e}. Install Node.js or set OXVELTE_NODE."))?;
        let mut input = child
            .stdin
            .take()
            .ok_or("Compiler runtime has no input pipe")?;
        write!(input, "{}\n{}", script.len(), script)
            .and_then(|_| input.flush())
            .map_err(|error| format!("Cannot initialize Node.js lint runtime: {error}"))?;
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
        let wire: WireResult = self.exchange(&key)?;
        let result = CompileResult {
            compiler_version: wire.compiler_version,
            svelte_major: wire.svelte_major,
            kind: wire.kind,
            warnings: convert_warnings(source, wire.warnings),
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
    fn exchange<T: DeserializeOwned>(&mut self, key: &str) -> Result<T, String> {
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
        let response: Response<T> = serde_json::from_str(&line)
            .map_err(|e| format!("Invalid Svelte compiler response: {e}"))?;
        if let Some(error) = response.error {
            return Err(error);
        }
        response
            .result
            .ok_or_else(|| "Compiler response did not contain a result".into())
    }
    fn callbacks(
        &mut self,
        source: &str,
        filename: Option<&str>,
        settings: Option<&Value>,
        warnings: &[Warning],
    ) -> Result<Vec<Warning>, String> {
        let request = CallbackRequest {
            operation: "callbacks",
            request: Request {
                source,
                filename,
                settings,
            },
            warnings: warnings
                .iter()
                .map(|warning| WireReport {
                    metadata: warning.metadata.clone(),
                    message: warning.message.clone(),
                    code: warning.code.clone(),
                    start: warning.span.map(|span| byte_utf16(source, span.start)),
                    end: warning.span.map(|span| byte_utf16(source, span.end)),
                })
                .collect(),
        };
        let key = serde_json::to_string(&request).map_err(|error| error.to_string())?;
        let wire: WireCallbacks = self.exchange(&key)?;
        Ok(convert_warnings(source, wire.warnings))
    }
}
fn convert_warnings(source: &str, warnings: Vec<WireWarning>) -> Vec<Warning> {
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
    warnings
        .into_iter()
        .map(|w| {
            let metadata = w.warning.metadata.clone();
            let raw = report(w.warning);
            Warning {
                metadata,
                message: raw.message,
                code: raw.code,
                span: raw.span,
                filtered: w.filtered,
                report: w.report.map(report),
            }
        })
        .collect()
}
pub(crate) fn byte_utf16(source: &str, byte: u32) -> usize {
    source
        .get(..byte as usize)
        .unwrap_or(source)
        .encode_utf16()
        .count()
}
pub(crate) fn utf16_byte(source: &str, index: usize) -> u32 {
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
    with_runtime(|runtime| runtime.compile(source, filename, settings))
}
/// Apply executable Svelte warning hooks only to warnings retained by the
/// native ignore and rule filters. Callback requests are intentionally uncached.
pub fn apply_warning_callbacks(
    source: &str,
    filename: Option<&str>,
    settings: Option<&Value>,
    warnings: &[Warning],
) -> Result<Vec<Warning>, String> {
    if warnings.is_empty() {
        return Ok(vec![]);
    }
    with_runtime(|runtime| runtime.callbacks(source, filename, settings, warnings))
}
/// Run opt-in type-aware checks through the shared Node process. TypeScript
/// programs are refreshed per request so unsaved source and imports stay current.
pub(crate) fn typescript_conditions(request: &Value) -> Result<Value, String> {
    let serialized = serde_json::to_string(request).map_err(|error| error.to_string())?;
    with_runtime(|runtime| runtime.exchange(&serialized))
}

fn with_runtime<T>(action: impl FnOnce(&mut Runtime) -> Result<T, String>) -> Result<T, String> {
    static RUNTIME: OnceLock<Mutex<Option<Runtime>>> = OnceLock::new();
    let mut state = RUNTIME
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "Svelte compiler runtime lock is poisoned")?;
    if state.is_none() {
        *state = Some(Runtime::start()?);
    }
    let result = action(state.as_mut().unwrap());
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
