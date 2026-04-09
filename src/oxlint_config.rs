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
//!    just keeps it alive for as long as the oxlint subprocess runs.
//!
//! Security note: NamedTempFile uses an unpredictable filename + O_EXCL
//! mode, preventing the classic `/tmp` symlink attack.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;

use crate::diagnostic::Severity;

/// Build an oxlint config and write it to a fresh temp file.
pub fn generate(rules: &[(&str, Severity)]) -> Result<tempfile::NamedTempFile> {
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
fn build_config_json(rules: &[(&str, Severity)]) -> Value {
    let plugins = collect_plugins(rules);

    let mut rule_map = serde_json::Map::new();
    for (key, severity) in rules {
        rule_map.insert((*key).to_string(), json!(severity_str(*severity)));
    }

    json!({
        "plugins": plugins,
        "rules": rule_map,
    })
}

/// Extract the unique plugin names referenced by the rule keys, sorted for
/// determinism. A config key like `typescript/no-explicit-any` implies the
/// `typescript` plugin; bare keys (`eqeqeq`) imply no plugin.
fn collect_plugins(rules: &[(&str, Severity)]) -> Vec<String> {
    let mut plugins: BTreeSet<String> = BTreeSet::new();
    for (key, _) in rules {
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
        let rules = [
            ("typescript/no-explicit-any", Severity::Error),
            ("typescript/no-non-null-assertion", Severity::Error),
            ("import/no-default-export", Severity::Error),
            ("eqeqeq", Severity::Error),
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
        let rules = [
            ("typescript/no-explicit-any", Severity::Error),
            ("eqeqeq", Severity::Warning),
        ];
        let config = build_config_json(&rules);
        assert_eq!(config["plugins"], json!(["typescript"]));
        assert_eq!(config["rules"]["typescript/no-explicit-any"], json!("error"));
        assert_eq!(config["rules"]["eqeqeq"], json!("warn"));
    }
}
