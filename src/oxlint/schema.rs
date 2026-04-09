//! Wire-format DTOs for oxlint's `--format json` output.
//!
//! Kept separate from the subprocess invocation so the deserialization rules
//! and the spawning logic can evolve independently. The serde structs use
//! `#[serde(flatten)] _extra: IgnoredAny` instead of declaring every oxlint
//! field — that way we accept new oxlint metadata fields without bit-rot,
//! while still failing loudly on schema-breaking changes to the fields we
//! DO read. `IgnoredAny` is zero-allocation: it consumes the JSON but stores
//! nothing, unlike `serde_json::Map<String, Value>` which heap-allocates.

use serde::de::IgnoredAny;
use serde::Deserialize;

/// Top-level oxlint JSON output envelope.
#[derive(Debug, Deserialize)]
pub struct OxlintOutput {
    #[serde(default)]
    pub diagnostics: Vec<OxlintDiag>,
    /// Catches all other top-level fields (number_of_files, threads_count, etc.)
    /// so future oxlint additions don't break parsing.
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// A single oxlint diagnostic — adapted from oxlint 1.59 JSON format.
#[derive(Debug, Deserialize)]
pub struct OxlintDiag {
    #[serde(default)]
    pub message: String,
    /// Rule identifier, e.g. "eslint(no-unused-vars)".
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub severity: OxlintSeverity,
    #[serde(default)]
    pub filename: String,
    /// Position labels — first label carries the primary span.
    #[serde(default)]
    pub labels: Vec<OxlintLabel>,
    /// Catches `causes`, `url`, `help`, `related`, etc.
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

#[non_exhaustive]
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OxlintSeverity {
    #[default]
    Error,
    Warning,
    Advice,
}

#[derive(Debug, Deserialize)]
pub struct OxlintLabel {
    #[serde(default)]
    pub span: OxlintSpan,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

#[derive(Debug, Deserialize, Default)]
pub struct OxlintSpan {
    #[serde(default)]
    pub line: usize,
    #[serde(default)]
    pub column: usize,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}
