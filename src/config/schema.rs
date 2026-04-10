//! TOML schema for `comply.toml`.
//!
//! Two-section structure:
//!
//! - `[rules.<rule-id>]` — per-rule overrides applied to every file
//!     - `disabled = true` — skip the rule entirely
//!     - `severity = "warning" | "error"` — override the rule's default
//!     - `<threshold-key> = <value>` — rule-specific knobs (max, min, etc.)
//!
//! - `[overrides."<glob>"]` — per-path overrides matched against the
//!   diagnostic's file path. Repeat for as many globs as needed.
//!     - `disable = ["rule-id-1", "rule-id-2"]` — silence these rules
//!       when the file matches the glob
//!     - Threshold overrides per rule are not supported in v2.6 — they
//!       collapse to global defaults inside the rule's check, since the
//!       check runs once per file before we know its glob bucket.
//!
//! Defaults are kept in `super::defaults` and merged into the user's
//! config at load time, so a user file with `[rules.max-function-lines]
//! max = 60` only changes that one knob, not the rest.

use serde::Deserialize;
use std::collections::HashMap;

/// Top-level shape of a `comply.toml` file. Both sections are optional;
/// an empty file is equivalent to "use every default".
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComplyToml {
    #[serde(default)]
    pub rules: HashMap<String, RuleConfig>,
    #[serde(default)]
    pub overrides: HashMap<String, OverrideConfig>,
}

/// Per-rule overrides. Every field is optional — only the ones the
/// user actually sets get written to TOML, and merging respects that.
///
/// `extra` captures any rule-specific threshold (`max`, `min`,
/// `min_arms`, `min_ops`, `min_line_length`, `threshold`, `values`,
/// etc.) so each rule can pull its own knob without us hardcoding
/// the schema for every rule.
///
/// `enabled` only matters for clippy lints that default to `allow`:
/// `enabled = true` flips one on by adding a `-W` flag to the clippy
/// invocation. Tree-sitter rules ignore the field — they run as long
/// as `disabled` isn't `true`.
///
/// `deny_unknown_fields` is intentionally OMITTED: `extra` uses
/// `#[serde(flatten)]` to capture rule-specific threshold knobs
/// without hardcoding each one. `deny_unknown_fields` would reject
/// those threshold keys *before* the flatten could catch them.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct RuleConfig {
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub severity: Option<SeverityToml>,
    #[serde(default, flatten)]
    pub extra: HashMap<String, toml::Value>,
}

/// Per-glob block. Use `disable = [...]` to silence specific rules
/// when the diagnostic's file path matches the glob key. The glob
/// syntax is the standard `globset` flavor: `**/*.rs`, `tests/**`,
/// `migrations/*.sql`, etc.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverrideConfig {
    #[serde(default)]
    pub disable: Vec<String>,
}

/// Severity values accepted in TOML. Mirrors `crate::diagnostic::Severity`
/// but kept separate so the wire format can evolve independently of the
/// internal type.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeverityToml {
    Warning,
    Error,
}

impl From<SeverityToml> for crate::diagnostic::Severity {
    fn from(s: SeverityToml) -> Self {
        match s {
            SeverityToml::Warning => crate::diagnostic::Severity::Warning,
            SeverityToml::Error => crate::diagnostic::Severity::Error,
        }
    }
}
