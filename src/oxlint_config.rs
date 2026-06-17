//! Dynamic oxlint config generator + secure temp file writer.
//!
//! The oxlint config used to live as a static `oxlintrc.json` embedded via
//! `include_str!`. Now it's generated at runtime from the registered
//! `Backend::Oxlint` rules in the comply rule registry, so the list of
//! enforced oxlint rules is authored in one place (the per-rule mod.rs
//! file) and the JSON config becomes a derived artifact.
//!
//! How it works:
//! 1. Caller collects `(config_key, severity)` pairs from the registry.
//! 2. `generate` extracts unique plugin names from the config keys, builds
//!    a JSON object with `plugins` + `rules`, and writes it to a fresh
//!    per-invocation temp file (`tempfile::NamedTempFile`).
//! 3. The returned NamedTempFile deletes itself on drop, so the caller
//!    keeps it alive for as long as the oxlint subprocess runs.
//!
//! Security note: NamedTempFile uses an unpredictable filename + O_EXCL
//! mode, preventing the classic `/tmp` symlink attack.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::Write;

use crate::diagnostic::Severity;

/// A rule entry bound for the generated oxlintrc: the rule key, its
/// severity, and optional ESLint-style options (`min`, `exceptions`,
/// etc.). When `options` is `Some`, oxlint receives the `[severity,
/// options]` tuple form; otherwise the bare severity string.
pub type RuleEntry<'a> = (&'a str, Severity, Option<Value>);

/// Build an oxlint config and write it to a fresh temp file.
pub fn generate(rules: &[RuleEntry<'_>]) -> Result<tempfile::NamedTempFile> {
    let config = build_config_json(rules);
    let serialized =
        serde_json::to_string_pretty(&config).context("failed to serialize oxlint config")?;

    let mut tmp = tempfile::Builder::new()
        .prefix("comply-")
        .suffix(".json")
        .tempfile()
        .context("failed to create temp oxlint config")?;
    tmp.write_all(serialized.as_bytes())
        .context("failed to write oxlint config to temp file")?;
    tmp.flush()
        .context("failed to flush temp oxlint config to disk")?;
    Ok(tmp)
}

/// Assemble the oxlint config JSON from a list of rule entries.
fn build_config_json(rules: &[RuleEntry<'_>]) -> Value {
    let plugins = collect_plugins(rules);

    let mut rule_map = serde_json::Map::new();
    for (key, severity, options) in rules {
        let entry = match options {
            Some(opts) => json!([severity_str(*severity), opts]),
            None => json!(severity_str(*severity)),
        };
        rule_map.insert((*key).to_string(), entry);
    }

    // oxlint turns the `correctness` category on by default, so listing a
    // plugin (to enable one or two of its rules explicitly) drags in every
    // other `correctness`-category rule that plugin ships. Those extras leak
    // out as un-remapped `eslint-plugin-foo(bar)` diagnostics with no comply
    // RuleMeta, remediation, or FP post-filter behind them. Disabling the
    // default category pins oxlint to exactly the rules comply registered;
    // an explicit `rules` entry overrides the category, so the rules we do
    // enable still run.
    json!({
        "plugins": plugins,
        "categories": { "correctness": "off" },
        "rules": rule_map,
    })
}

/// Extract the unique plugin names referenced by the rule keys, sorted for
/// determinism. A config key like `typescript/no-explicit-any` implies the
/// `typescript` plugin; bare keys (`eqeqeq`) imply no plugin.
fn collect_plugins(rules: &[RuleEntry<'_>]) -> Vec<String> {
    let mut plugins: BTreeSet<String> = BTreeSet::new();
    for (key, _, _) in rules {
        if let Some((plugin, _)) = key.split_once('/') {
            plugins.insert(plugin.to_string());
        }
    }
    plugins.into_iter().collect()
}

fn severity_str(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_unique_plugins_from_rule_keys() {
        let rules: [RuleEntry; 4] = [
            ("typescript/no-explicit-any", Severity::Error, None),
            ("typescript/no-non-null-assertion", Severity::Error, None),
            ("import/no-default-export", Severity::Error, None),
            ("eqeqeq", Severity::Error, None),
        ];
        let plugins = collect_plugins(&rules);
        assert_eq!(plugins, vec!["import", "typescript"]);
    }

    #[test]
    fn severity_maps_to_oxlint_strings() {
        assert_eq!(severity_str(Severity::Error), "error");
        assert_eq!(severity_str(Severity::Warning), "warn");
    }

    #[test]
    fn build_config_emits_rules_and_plugins() {
        let rules: [RuleEntry; 2] = [
            ("typescript/no-explicit-any", Severity::Error, None),
            ("eqeqeq", Severity::Warning, None),
        ];
        let config = build_config_json(&rules);
        assert_eq!(config["plugins"], json!(["typescript"]));
        assert_eq!(
            config["rules"]["typescript/no-explicit-any"],
            json!("error")
        );
        assert_eq!(config["rules"]["eqeqeq"], json!("warn"));
    }

    #[test]
    fn build_config_disables_default_correctness_category() {
        // Regression for #4010: enabling a plugin (e.g. `jest`, for the two
        // jest rules comply delegates) must NOT drag in the plugin's whole
        // `correctness` category. oxlint runs that category by default, so the
        // generated config has to switch it off — otherwise rules like
        // `jest/no-standalone-expect` fire as un-remapped FPs (1734 on a
        // Vitest + @fast-check/vitest repo).
        let rules: [RuleEntry; 1] = [("jest/no-export", Severity::Error, None)];
        let config = build_config_json(&rules);
        assert_eq!(config["categories"]["correctness"], json!("off"));
        // The explicitly-listed rule survives the category being off.
        assert_eq!(config["rules"]["jest/no-export"], json!("error"));
    }

    #[test]
    fn build_config_emits_options_tuple_form_when_present() {
        // ESLint / oxlint accept either a severity string OR a
        // `[severity, optionsObject]` tuple. When comply has options
        // to propagate (`id-length` with exceptions, etc.), we must
        // emit the tuple form — the bare string would be an error.
        let options = json!({"min": 2, "exceptions": ["t"]});
        let rules: [RuleEntry; 1] = [("id-length", Severity::Error, Some(options.clone()))];
        let config = build_config_json(&rules);
        assert_eq!(config["rules"]["id-length"], json!(["error", options]));
    }
}
