//! Per-rule option derivation for the generated oxlintrc.
//!
//! Some oxlint rules accept options beyond just the severity level
//! (`id-length` takes `min` / `exceptions` / `exceptionPatterns`, etc.).
//! This module translates the user's `[rules."<id>"]` TOML section
//! into the JSON shape oxlint expects.
//!
//! Rules not listed here stay severity-only — they get `"error"` or
//! `"warn"` rather than the `[severity, options]` tuple form.

use serde_json::{json, Value};

use crate::config::Config;

/// Return the options object to pass to oxlint for `rule_id`, or
/// `None` if this rule is severity-only.
#[must_use]
pub fn for_rule(rule_id: &str, config: &Config) -> Option<Value> {
    match rule_id {
        "id-length" => Some(id_length_options(config)),
        _ => None,
    }
}

/// Options for `id-length` — mirrors the ESLint rule shape:
/// <https://eslint.org/docs/rules/id-length>. Keys not configured in
/// `comply.toml` are omitted so oxlint falls back to its own defaults
/// for them.
///
/// Comply TOML keys → oxlint JSON keys:
/// - `min` → `min` (default 2, matches comply's baseline)
/// - `max` → `max`
/// - `exceptions` → `exceptions` (string list of identifiers to allow)
/// - `exception_patterns` → `exceptionPatterns` (regex list)
/// - `properties` → `properties` (`"always"` | `"never"`)
fn id_length_options(config: &Config) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("min".into(), json!(config.threshold("id-length", "min", 2)));

    if let Some(max) = config.optional_threshold("id-length", "max") {
        obj.insert("max".into(), json!(max));
    }

    let exceptions = config.string_list("id-length", "exceptions");
    if !exceptions.is_empty() {
        obj.insert("exceptions".into(), json!(exceptions));
    }

    let patterns = config.string_list("id-length", "exception_patterns");
    if !patterns.is_empty() {
        obj.insert("exceptionPatterns".into(), json!(patterns));
    }

    if let Some(props) = config.optional_string("id-length", "properties") {
        obj.insert("properties".into(), json!(props));
    }

    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{default_static_config, Config};

    fn cfg_with(toml: &str) -> Config {
        Config::from_toml_str(toml).expect("valid test config")
    }

    #[test]
    fn unknown_rule_has_no_options() {
        assert!(for_rule("no-await-in-loop", default_static_config()).is_none());
    }

    #[test]
    fn id_length_default_emits_min_only() {
        let opts = for_rule("id-length", default_static_config()).expect("id-length has options");
        assert_eq!(opts["min"], json!(2));
        assert!(opts.get("exceptions").is_none());
        assert!(opts.get("exceptionPatterns").is_none());
    }

    #[test]
    fn id_length_propagates_exceptions_from_toml() {
        let cfg = cfg_with(
            r#"
[rules."id-length"]
exceptions = ["t", "x", "i"]
"#,
        );
        let opts = for_rule("id-length", &cfg).expect("options");
        assert_eq!(opts["exceptions"], json!(["t", "x", "i"]));
    }

    #[test]
    fn id_length_propagates_exception_patterns() {
        let cfg = cfg_with(
            r#"
[rules."id-length"]
exception_patterns = ["^[A-Z]$"]
"#,
        );
        let opts = for_rule("id-length", &cfg).expect("options");
        assert_eq!(opts["exceptionPatterns"], json!(["^[A-Z]$"]));
    }

    #[test]
    fn id_length_custom_min_overrides_default() {
        let cfg = cfg_with(
            r#"
[rules."id-length"]
min = 3
"#,
        );
        let opts = for_rule("id-length", &cfg).expect("options");
        assert_eq!(opts["min"], json!(3));
    }
}
