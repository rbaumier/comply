//! Project configuration loaded from `comply.toml`.
//!
//! Lookup order at startup:
//!   1. Walk up from the current working directory looking for the
//!      nearest `comply.toml`.
//!   2. If found, parse it and merge the user's overrides on top of
//!      the hardcoded defaults from `defaults::build_default_config`.
//!   3. If not found, use defaults verbatim.
//!
//! Public API for the rest of the codebase:
//!   - `Config::load_from_cwd()` — entry point used by `main`
//!   - `Config::default()` — defaults only, used by tests
//!   - `Config::is_rule_enabled(rule_id, file_path)` — combine global
//!     `disabled = true` with per-glob `disable = [...]` overrides
//!   - `Config::severity_for(rule_id)` — global severity override
//!   - `Config::threshold(rule_id, key, fallback)` — typed threshold
//!     accessor used by every rule that has a knob
//!   - `Config::print_default_toml()` — pretty-print the defaults so
//!     `comply config init` can dump them to disk

mod defaults;
mod schema;

pub use schema::{ComplyToml, RuleConfig};

/// Process-wide default config used by `CheckCtx::for_test` so unit
/// tests don't have to construct one. Initialized lazily on first use.
/// Test-only — production code threads a real `Config` through `engine`.
#[cfg(test)]
pub fn default_static_config() -> &'static Config {
    use std::sync::OnceLock;
    static DEFAULT: OnceLock<Config> = OnceLock::new();
    DEFAULT.get_or_init(Config::default)
}

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

use crate::diagnostic::Severity;

/// File name we look for. Always lowercase, never `.comply.toml`
/// (no dot prefix) so it shows up in default file listings.
pub const CONFIG_FILE_NAME: &str = "comply.toml";

/// Resolved configuration: defaults merged with the user's `comply.toml`,
/// plus a precompiled glob matcher for the per-path overrides.
#[derive(Debug)]
pub struct Config {
    raw: ComplyToml,
    /// Compiled globs in the order they appear in the TOML. Each
    /// `disable_lists[i]` is the rule-id list to silence when the
    /// matcher's i-th glob fires for a path.
    glob_matcher: GlobSet,
    disable_lists: Vec<Vec<String>>,
}

impl Config {
    /// Defaults-only config — used by tests and as the base layer
    /// when no `comply.toml` is found in the project tree.
    pub fn default() -> Self {
        // defaults::build_default_config() is statically known to compile;
        // a panic here means a programmer bug in the defaults table, not a
        // runtime condition.
        Self::from_raw(defaults::build_default_config()) // comply-ignore: rust-no-unwrap — defaults are static.
            .expect("default config must compile")
    }

    /// Walk up from `start_dir` looking for the nearest `comply.toml`.
    /// If one is found, layer the user's overrides on top of the
    /// defaults; otherwise return the defaults verbatim.
    pub fn load_from(start_dir: &Path) -> Result<Self> {
        let Some(path) = find_comply_toml(start_dir) else {
            return Ok(Self::default());
        };
        let user_text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let user: ComplyToml = toml::from_str(&user_text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let merged = merge(defaults::build_default_config(), user);
        Self::from_raw(merged)
    }

    /// Return the resolved config as TOML text — used by
    /// `comply config init` to seed a project's `comply.toml`.
    #[must_use]
    pub fn print_default_toml() -> String {
        let cfg = defaults::build_default_config();
        toml::to_string_pretty(&cfg)
            .map(|body| format!("# comply.toml — generated defaults\n{body}"))
            .unwrap_or_else(|_| "# failed to render defaults\n".to_string())
    }

    /// True if `rule_id` is enabled for `file_path`. Combines:
    ///   - global `[rules.<id>] disabled = true` (kills the rule everywhere)
    ///   - per-glob `[overrides."<g>"] disable = [<id>]` (kills it for matches)
    #[must_use]
    pub fn is_rule_enabled(&self, rule_id: &str, file_path: &Path) -> bool {
        if let Some(rule) = self.raw.rules.get(rule_id)
            && rule.disabled == Some(true)
        {
            return false;
        }
        for idx in self.glob_matcher.matches(file_path) {
            if self.disable_lists[idx].iter().any(|d| d == rule_id) {
                return false;
            }
        }
        true
    }

    /// Override severity for a rule, or `None` if the user didn't set one.
    #[must_use]
    pub fn severity_for(&self, rule_id: &str) -> Option<Severity> {
        self.raw
            .rules
            .get(rule_id)
            .and_then(|r| r.severity)
            .map(Into::into)
    }

    /// Iterate over every `[rules.<id>]` block the user (or the
    /// defaults) has configured. Returns `(rule_id, &RuleConfig)`
    /// pairs. Used by the clippy module to discover which lints to
    /// enable explicitly and which thresholds to write into the
    /// generated `clippy.toml`.
    pub fn iter_rules(&self) -> impl Iterator<Item = (&str, &RuleConfig)> {
        self.raw.rules.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Read a numeric threshold for `rule_id`. The hardcoded `fallback`
    /// is the value the rule should use when neither the user nor the
    /// defaults provide one — keeping it at the call site means a rule
    /// stays self-contained even if the config layer goes wrong.
    #[must_use]
    pub fn threshold(&self, rule_id: &str, key: &str, fallback: usize) -> usize {
        self.raw
            .rules
            .get(rule_id)
            .and_then(|r| r.extra.get(key))
            .and_then(toml::Value::as_integer)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(fallback)
    }

    fn from_raw(raw: ComplyToml) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        let mut disable_lists: Vec<Vec<String>> = Vec::new();
        for (pattern, override_cfg) in &raw.overrides {
            let glob = Glob::new(pattern)
                .with_context(|| format!("invalid glob in [overrides.\"{pattern}\"]"))?;
            builder.add(glob);
            disable_lists.push(override_cfg.disable.clone());
        }
        let glob_matcher = builder
            .build()
            .context("failed to compile [overrides] globs")?;
        Ok(Self {
            raw,
            glob_matcher,
            disable_lists,
        })
    }
}

/// Walk up from `start` looking for `comply.toml`. Returns the absolute
/// path to the file if one is found, or `None` if we hit the filesystem
/// root without finding one.
fn find_comply_toml(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        let candidate = cur.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Layer `user` on top of `base`. Per-rule overrides merge field-by-field:
/// the user's `disabled`, `severity`, and individual `extra` knobs win,
/// while everything they didn't touch falls back to the default.
/// Per-path overrides replace whatever was at that glob.
fn merge(mut base: ComplyToml, user: ComplyToml) -> ComplyToml {
    for (rule_id, user_rule) in user.rules {
        let entry = base.rules.entry(rule_id).or_default();
        if let Some(d) = user_rule.disabled {
            entry.disabled = Some(d);
        }
        if let Some(e) = user_rule.enabled {
            entry.enabled = Some(e);
        }
        if let Some(s) = user_rule.severity {
            entry.severity = Some(s);
        }
        for (k, v) in user_rule.extra {
            entry.extra.insert(k, v);
        }
    }
    for (glob, override_cfg) in user.overrides {
        base.overrides.insert(glob, override_cfg);
    }
    base
}

// Workaround: serde serialization needs ComplyToml to be Serialize for
// `print_default_toml`. Wire it up here rather than in schema.rs so the
// public schema struct stays minimal.
mod serialize_impl {
    use super::schema::{ComplyToml, OverrideConfig, RuleConfig, SeverityToml};
    use serde::ser::SerializeStruct;
    use serde::{Serialize, Serializer};

    impl Serialize for ComplyToml {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut state = s.serialize_struct("ComplyToml", 2)?;
            state.serialize_field("rules", &self.rules)?;
            state.serialize_field("overrides", &self.overrides)?;
            state.end()
        }
    }

    impl Serialize for RuleConfig {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            // Inline `extra` into the rule's table so it reads as
            // `[rules.foo] max = 30` instead of `[rules.foo.extra]`.
            use serde::ser::SerializeMap;
            let mut len = self.extra.len();
            if self.disabled.is_some() {
                len += 1;
            }
            if self.severity.is_some() {
                len += 1;
            }
            let mut map = s.serialize_map(Some(len))?;
            if let Some(d) = self.disabled {
                map.serialize_entry("disabled", &d)?;
            }
            if let Some(sev) = self.severity {
                map.serialize_entry("severity", &sev)?;
            }
            for (k, v) in &self.extra {
                map.serialize_entry(k, v)?;
            }
            map.end()
        }
    }

    impl Serialize for OverrideConfig {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut state = s.serialize_struct("OverrideConfig", 1)?;
            state.serialize_field("disable", &self.disable)?;
            state.end()
        }
    }

    impl Serialize for SeverityToml {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            match self {
                SeverityToml::Warning => s.serialize_str("warning"),
                SeverityToml::Error => s.serialize_str("error"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn default_config_returns_known_thresholds() {
        let cfg = Config::default();
        assert_eq!(cfg.threshold("max-function-lines", "max", 999), 30);
        assert_eq!(cfg.threshold("max-file-lines", "max", 999), 200);
    }

    #[test]
    fn threshold_uses_fallback_when_unknown() {
        let cfg = Config::default();
        assert_eq!(cfg.threshold("does-not-exist", "max", 42), 42);
    }

    #[test]
    fn user_config_overrides_threshold() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [rules.max-function-lines]
            max = 80
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert_eq!(cfg.threshold("max-function-lines", "max", 999), 80);
        // Other defaults still intact.
        assert_eq!(cfg.threshold("max-file-lines", "max", 999), 200);
    }

    #[test]
    fn disabled_rule_is_filtered_globally() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [rules.max-function-lines]
            disabled = true
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert!(!cfg.is_rule_enabled("max-function-lines", Path::new("src/foo.rs")));
        assert!(cfg.is_rule_enabled("max-file-lines", Path::new("src/foo.rs")));
    }

    #[test]
    fn glob_override_disables_rule_only_for_matching_paths() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [overrides."tests/**"]
            disable = ["rust-no-unwrap"]
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert!(!cfg.is_rule_enabled("rust-no-unwrap", Path::new("tests/foo.rs")));
        assert!(cfg.is_rule_enabled("rust-no-unwrap", Path::new("src/foo.rs")));
    }

    #[test]
    fn severity_override_returns_user_choice() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("comply.toml");
        fs::write(
            &cfg_path,
            r#"
            [rules.max-function-lines]
            severity = "error"
            "#,
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert!(matches!(
            cfg.severity_for("max-function-lines"),
            Some(Severity::Error)
        ));
        assert!(cfg.severity_for("max-file-lines").is_none());
    }

    #[test]
    fn missing_config_falls_back_to_defaults() {
        let tmp = TempDir::new().unwrap();
        // No comply.toml in tmp or any ancestor (we may walk up to a
        // real one, that's fine — what we test is "no panic").
        let cfg = Config::load_from(tmp.path()).unwrap();
        // The default for max-function-lines is 30, regardless of
        // whether we walked into a real workspace.
        let _ = cfg.threshold("max-function-lines", "max", 30);
    }

    #[test]
    fn print_default_toml_renders_expected_section() {
        let text = Config::print_default_toml();
        // toml::to_string_pretty inlines empty parents and emits the
        // dotted form, so we expect `[rules.max-function-lines]`.
        assert!(text.contains("[rules.max-function-lines]"));
        assert!(text.contains("max ="));
    }
}
