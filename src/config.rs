//! Oxvelte configuration file support.
//!
//! Config file: `oxvelte.config.json` (project root)
//!
//! ```json
//! {
//!   "rules": {
//!     "svelte/no-at-html-tags": "error",
//!     "svelte/button-has-type": ["warn", { "button": false }],
//!     "svelte/no-inline-styles": "off"
//!   },
//!   "settings": {
//!     "svelte": {
//!       "kit": {
//!         "files": { "routes": "src/routes" }
//!       }
//!     }
//!   }
//! }
//! ```
//!
//! Rule severity: "off" | "warn" | "error" (or 0 | 1 | 2)
//! Rule options: ["error", { ...options }]

use crate::linter::RuleConfig;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Parsed oxvelte configuration.
#[derive(Debug, Clone, Default)]
pub struct OxvelteConfig {
    /// Rule name → (enabled, severity, options)
    pub rules: HashMap<String, RuleEntry>,
    /// Global settings (e.g. svelte.kit.files.routes)
    pub settings: Option<Value>,
    /// Glob patterns for custom JS rule files (e.g. `["./rules/*.js"]`)
    pub custom_rules: Vec<String>,
}

/// A single rule's configuration.
#[derive(Debug, Clone)]
pub struct RuleEntry {
    /// "off", "warn", or "error"
    pub severity: String,
    /// Rule-specific options (if any)
    pub options: Option<Value>,
}

impl RuleEntry {
    pub fn is_off(&self) -> bool {
        self.severity == "off" || self.severity == "0"
    }
}

impl OxvelteConfig {
    /// Load config from the nearest `oxvelte.config.json` walking up from `start_dir`.
    ///
    /// Kept for callers that only need the config values; ignores read/parse
    /// errors and the config's location. Prefer [`OxvelteConfig::load_with_dir`].
    pub fn load(start_dir: &Path) -> Self {
        Self::load_with_dir(start_dir)
            .map(|(config, _)| config)
            .unwrap_or_default()
    }

    /// Load config from the nearest `oxvelte.config.json` walking up from
    /// `start_dir`, also returning the directory that contains it (so
    /// relative paths in the config, such as `customRules` globs, resolve
    /// against the config's own location, not the lint path's directory).
    ///
    /// A config file that exists but cannot be read or parsed is a hard
    /// error: silently falling back to a parent config or the defaults would
    /// lint the project with the wrong rules and no indication.
    pub fn load_with_dir(start_dir: &Path) -> Result<(Self, PathBuf), String> {
        let mut dir = start_dir;
        loop {
            let config_path = dir.join("oxvelte.config.json");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)
                    .map_err(|e| format!("Error reading config {}: {e}", config_path.display()))?;
                let config = Self::parse(&content)
                    .map_err(|e| format!("Error parsing config {}: {e}", config_path.display()))?;
                return Ok((config, dir.to_path_buf()));
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
        Ok((Self::default(), start_dir.to_path_buf()))
    }

    /// Parse config from a JSON string.
    pub fn parse(json_str: &str) -> Result<Self, String> {
        let value: Value =
            serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;

        let mut config = Self::default();

        // Parse rules
        if let Some(rules_obj) = value.get("rules").and_then(|v| v.as_object()) {
            for (name, val) in rules_obj {
                let entry = parse_rule_entry(val);
                config.rules.insert(name.clone(), entry);
            }
        }

        // Parse settings
        config.settings = value.get("settings").cloned();

        // Parse customRules
        if let Some(arr) = value.get("customRules").and_then(|v| v.as_array()) {
            config.custom_rules = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }

        Ok(config)
    }

    /// Get the RuleConfig for a specific rule name.
    pub fn rule_config(&self, rule_name: &str) -> RuleConfig {
        if let Some(entry) = self.rules.get(rule_name) {
            RuleConfig {
                options: entry.options.clone(),
                settings: self.settings.clone(),
            }
        } else {
            RuleConfig {
                options: None,
                settings: self.settings.clone(),
            }
        }
    }

    /// Check if a rule is explicitly disabled.
    pub fn is_rule_off(&self, rule_name: &str) -> bool {
        self.rules.get(rule_name).is_some_and(|e| e.is_off())
    }

    /// Check if a rule is explicitly enabled, at any severity other than off.
    pub fn is_rule_on(&self, rule_name: &str) -> bool {
        self.rules.get(rule_name).is_some_and(|e| !e.is_off())
    }

    /// Convert from an ESLint-style config (flat or legacy).
    pub fn from_eslint(json_str: &str) -> Result<Self, String> {
        let value: Value =
            serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;

        let mut config = Self::default();

        // Handle flat config array: [{ rules: {...} }, ...]
        if let Some(arr) = value.as_array() {
            for item in arr {
                merge_eslint_object(item, &mut config);
            }
        } else {
            // Legacy .eslintrc object
            merge_eslint_object(&value, &mut config);
        }

        Ok(config)
    }

    /// Serialize to oxvelte.config.json format.
    pub fn to_json(&self) -> String {
        let mut obj = serde_json::Map::new();

        if !self.rules.is_empty() {
            let mut rules = serde_json::Map::new();
            let mut sorted_rules: Vec<_> = self.rules.iter().collect();
            sorted_rules.sort_by(|(a, _), (b, _)| a.cmp(b));
            for (name, entry) in sorted_rules {
                let val = if let Some(opts) = &entry.options {
                    let mut arr = vec![Value::String(entry.severity.clone())];
                    if let Some(opts) = opts.as_array() {
                        arr.extend(opts.iter().cloned());
                    } else {
                        arr.push(opts.clone());
                    }
                    Value::Array(arr)
                } else {
                    Value::String(entry.severity.clone())
                };
                rules.insert(name.clone(), val);
            }
            obj.insert("rules".to_string(), Value::Object(rules));
        }

        if let Some(settings) = &self.settings {
            obj.insert("settings".to_string(), settings.clone());
        }

        if !self.custom_rules.is_empty() {
            obj.insert(
                "customRules".to_string(),
                Value::Array(
                    self.custom_rules
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }

        serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default()
    }
}

fn parse_rule_entry(val: &Value) -> RuleEntry {
    match val {
        // "off" | "warn" | "error"
        Value::String(s) => RuleEntry {
            severity: normalize_severity(s),
            options: None,
        },
        // 0 | 1 | 2
        Value::Number(n) => RuleEntry {
            severity: match n.as_u64() {
                Some(0) => "off".to_string(),
                Some(1) => "warn".to_string(),
                _ => "error".to_string(),
            },
            options: None,
        },
        // ["error", option1, option2, ...]
        Value::Array(arr) => {
            let severity = arr
                .first()
                .map(|v| match v {
                    Value::String(s) => normalize_severity(s),
                    Value::Number(n) => match n.as_u64() {
                        Some(0) => "off".to_string(),
                        Some(1) => "warn".to_string(),
                        _ => "error".to_string(),
                    },
                    _ => "error".to_string(),
                })
                .unwrap_or_else(|| "error".to_string());
            let options = if arr.len() > 1 {
                Some(Value::Array(arr.iter().skip(1).cloned().collect()))
            } else {
                None
            };
            RuleEntry { severity, options }
        }
        _ => RuleEntry {
            severity: "error".to_string(),
            options: None,
        },
    }
}

fn normalize_severity(s: &str) -> String {
    match s {
        "0" | "off" => "off".to_string(),
        "1" | "warn" => "warn".to_string(),
        _ => "error".to_string(),
    }
}

fn merge_eslint_object(obj: &Value, config: &mut OxvelteConfig) {
    // Extract rules — only keep svelte/* rules
    if let Some(rules) = obj.get("rules").and_then(|v| v.as_object()) {
        for (name, val) in rules {
            if name.starts_with("svelte/") {
                config.rules.insert(name.clone(), parse_rule_entry(val));
            }
        }
    }

    // Extract settings
    if let Some(settings) = obj.get("settings") {
        config.settings = Some(settings.clone());
    }

    // Handle overrides array (legacy ESLint)
    if let Some(overrides) = obj.get("overrides").and_then(|v| v.as_array()) {
        for override_obj in overrides {
            // Check if this override applies to *.svelte files
            let applies_to_svelte = override_obj
                .get("files")
                .and_then(|f| f.as_array())
                .is_some_and(|files| {
                    files
                        .iter()
                        .any(|f| f.as_str().is_some_and(|s| s.contains(".svelte")))
                });
            if applies_to_svelte {
                merge_eslint_object(override_obj, config);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let json = r#"{ "rules": { "svelte/no-at-html-tags": "error" } }"#;
        let config = OxvelteConfig::parse(json).unwrap();
        assert_eq!(config.rules["svelte/no-at-html-tags"].severity, "error");
        assert!(!config.is_rule_off("svelte/no-at-html-tags"));
    }

    #[test]
    fn test_parse_with_options() {
        let json = r#"{ "rules": { "svelte/button-has-type": ["warn", { "button": false }] } }"#;
        let config = OxvelteConfig::parse(json).unwrap();
        let entry = &config.rules["svelte/button-has-type"];
        assert_eq!(entry.severity, "warn");
        assert!(entry.options.is_some());
        assert_eq!(
            config.rule_config("svelte/button-has-type").options,
            Some(serde_json::json!([{ "button": false }]))
        );
    }

    #[test]
    fn test_parse_preserves_multiple_options() {
        let json = r#"{ "rules": { "svelte/example": ["error", "always", { "except": true }] } }"#;
        let config = OxvelteConfig::parse(json).unwrap();
        assert_eq!(
            config.rule_config("svelte/example").options,
            Some(serde_json::json!(["always", { "except": true }]))
        );
    }

    #[test]
    fn test_parse_off() {
        let json = r#"{ "rules": { "svelte/no-inline-styles": "off" } }"#;
        let config = OxvelteConfig::parse(json).unwrap();
        assert!(config.is_rule_off("svelte/no-inline-styles"));
    }

    #[test]
    fn test_to_json_round_trips_custom_rules() {
        let json =
            r#"{ "rules": { "custom/a": "error" }, "customRules": ["./rules/a.js", "./rules/*.js"] }"#;
        let config = OxvelteConfig::parse(json).unwrap();
        let serialized = config.to_json();
        let reparsed = OxvelteConfig::parse(&serialized).unwrap();
        assert_eq!(
            reparsed.custom_rules,
            vec!["./rules/a.js".to_string(), "./rules/*.js".to_string()],
            "customRules must survive a parse/serialize round-trip: {serialized}"
        );
    }

    #[test]
    fn test_from_eslint_legacy() {
        let json = r#"{
            "plugins": ["svelte"],
            "rules": {
                "svelte/no-at-html-tags": "warn",
                "no-console": "error",
                "svelte/button-has-type": ["error", { "button": false }]
            },
            "settings": {
                "svelte": { "kit": { "files": { "routes": "src/routes" } } }
            }
        }"#;
        let config = OxvelteConfig::from_eslint(json).unwrap();
        // Only svelte rules kept
        assert_eq!(config.rules.len(), 2);
        assert!(config.rules.contains_key("svelte/no-at-html-tags"));
        assert!(config.rules.contains_key("svelte/button-has-type"));
        assert!(!config.rules.contains_key("no-console"));
        // Settings preserved
        assert!(config.settings.is_some());
    }

    #[test]
    fn test_roundtrip() {
        let json = r#"{ "rules": { "svelte/no-at-html-tags": "error", "svelte/button-has-type": ["warn", { "button": false }] } }"#;
        let config = OxvelteConfig::parse(json).unwrap();
        let output = config.to_json();
        let reparsed = OxvelteConfig::parse(&output).unwrap();
        assert_eq!(reparsed.rules.len(), 2);
    }

    #[test]
    fn load_with_dir_returns_the_configs_own_directory() {
        let base = std::env::temp_dir()
            .join(format!("oxvelte-cfg-dir-{}", std::process::id()));
        let child = base.join("sub");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(base.join("oxvelte.config.json"), "{ \"rules\": {} }").unwrap();
        let (_config, dir) = OxvelteConfig::load_with_dir(&child).unwrap();
        assert_eq!(
            dir, base,
            "customRules globs must resolve against the config's directory"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn load_with_dir_errors_on_invalid_discovered_config() {
        let base = std::env::temp_dir()
            .join(format!("oxvelte-cfg-bad-{}", std::process::id()));
        let child = base.join("sub");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(base.join("oxvelte.config.json"), "{ not json").unwrap();
        // An invalid config must surface, not be skipped silently.
        assert!(OxvelteConfig::load_with_dir(&child).is_err());
        std::fs::remove_dir_all(&base).ok();
    }
}
