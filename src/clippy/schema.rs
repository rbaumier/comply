//! Wire-format DTOs for `cargo --message-format=json` output.
//!
//! Cargo emits one JSON object per line (JSONL stream): one for each
//! `compiler-message` (the diagnostic we want), one for each
//! `compiler-artifact`, one for `build-finished`, etc. We only care
//! about `reason == "compiler-message"`.
//!
//! Inside a compiler-message, the `message` field is rustc's diagnostic
//! format (also used by `rustc --error-format=json`). The structure is
//! stable enough to depend on but verbose, so we use `IgnoredAny` for
//! the fields we don't need.

use serde::de::IgnoredAny;
use serde::Deserialize;

/// Top-level cargo message envelope. We only deserialize the `reason`
/// discriminator and the `message` payload — every other field of the
/// envelope (target, package_id, manifest_path, etc.) is ignored.
#[derive(Deserialize)]
pub struct CargoMessage {
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: Option<RustcDiagnostic>,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// A rustc/clippy diagnostic. Mirrors what `rustc --error-format=json`
/// produces inside a cargo `compiler-message`.
#[derive(Deserialize)]
pub struct RustcDiagnostic {
    #[serde(default)]
    pub message: String,
    /// `code.code` is the lint name, e.g. `clippy::unwrap_used`.
    /// rustc messages without a code (build errors, parse errors)
    /// have `code: null`.
    #[serde(default)]
    pub code: Option<RustcCode>,
    #[serde(default)]
    pub level: RustcLevel,
    #[serde(default)]
    pub spans: Vec<RustcSpan>,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

#[derive(Deserialize)]
pub struct RustcCode {
    #[serde(default)]
    pub code: String,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RustcLevel {
    #[default]
    Note,
    Help,
    Warning,
    Error,
    #[serde(rename = "failure-note")]
    FailureNote,
}

#[derive(Deserialize)]
pub struct RustcSpan {
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub line_start: usize,
    #[serde(default)]
    pub column_start: usize,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}
