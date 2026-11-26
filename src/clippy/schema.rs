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

use serde::Deserialize;
use serde::de::IgnoredAny;

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
/// Top-level cargo message envelope.
/// We read `reason`, `message`, `manifest_path` and `target`.
/// Cargo's other envelope fields are ignored.
#[derive(Debug, Deserialize)]
pub struct CargoMessage {
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: Option<RustcDiagnostic>,
    /// Absolute `Cargo.toml` of the crate cargo built.
    /// A `compiler-artifact` and a `compiler-message` carry it.
    #[serde(default)]
    pub manifest_path: Option<String>,
    /// The target that artifact or message belongs to.
    #[serde(default)]
    pub target: Option<CargoTarget>,
    /// The files a `compiler-artifact` wrote, its dep-info beside them.
    #[serde(default)]
    pub filenames: Vec<String>,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// One target of a package, as cargo names it.
#[derive(Debug, Deserialize)]
pub struct CargoTarget {
    /// `lib`, `bin`, `test`, `proc-macro`, `custom-build`…
    #[serde(default)]
    pub kind: Vec<String>,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
/// A rustc/clippy diagnostic. Mirrors what `rustc --error-format=json`
/// produces inside a cargo `compiler-message`.
#[derive(Debug, Deserialize)]
pub struct RustcDiagnostic {
    #[serde(default)]
    pub message: String,
    /// `error`, `warning`, `note`, `help`.
    /// An `error` without a `clippy::` code stopped the build.
    /// Clippy linted nothing past it.
    /// A `children` entry carries `note`, `help` or `warning`.
    #[serde(default)]
    pub level: String,
    /// rustc's own rendering, file and line included.
    #[serde(default)]
    pub rendered: Option<String>,
    /// `code.code` is the lint name, e.g. `clippy::unwrap_used`.
    /// rustc messages without a code (build errors, parse errors)
    /// have `code: null`.
    #[serde(default)]
    pub code: Option<RustcCode>,
    #[serde(default)]
    pub spans: Vec<RustcSpan>,
    /// A `disallowed-*` entry's `reason` arrives as a `note` child.
    #[serde(default)]
    pub children: Vec<RustcDiagnostic>,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
pub struct RustcCode {
    #[serde(default)]
    pub code: String,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
pub struct RustcSpan {
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub line_start: usize,
    #[serde(default)]
    pub column_start: usize,
    #[serde(default)]
    pub is_primary: bool,
    /// The source lines the span covers.
    #[serde(default)]
    pub text: Vec<RustcSpanLine>,
    /// The fix a `help` child proposes, e.g. the key a typo meant.
    #[serde(default)]
    pub suggested_replacement: Option<String>,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
pub struct RustcSpanLine {
    #[serde(default)]
    pub text: String,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
pub struct Metadata {
    /// Under `--no-deps`, the workspace members alone.
    pub packages: Vec<MetadataPackage>,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}

/// External wire format mirror — see comply:rust-serde-deny-unknown-fields.
#[derive(Debug, Deserialize)]
pub struct MetadataPackage {
    pub manifest_path: std::path::PathBuf,
    #[serde(default, flatten)]
    pub _extra: IgnoredAny,
}
