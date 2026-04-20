//! id-length — native implementation that names the offending identifier.
//!
//! Replaces the previous oxlint `id-length` + clippy `min_ident_chars`
//! delegation because the upstream diagnostics hid the actual
//! identifier behind a generic message (`Identifier name is too short
//! (< 2)`). Our version walks the tree-sitter AST for TS/JS/TSX and
//! Rust, only flags *binding* positions (declarations, not usages),
//! and emits `` Identifier `t` is too short (< 2) `` so a reader sees
//! the culprit without opening the file.
//!
//! Options are read from `[rules.id-length]` in `comply.toml`:
//!   - `min` (default `2`)
//!   - `exceptions` — exact-match allowlist (e.g. `["t", "T"]`)
//!   - `exception_patterns` — regex allowlist (e.g. `["^[A-Z]$"]`)

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "id-length",
    description: "Identifier names shorter than `min` hide intent.",
    remediation: "Rename to a full word — `createdAt` not `d`, `userCount` \
                  not `n`. Allow-list conventional short names in \
                  `comply.toml`:\n\n\
                  [rules.id-length]\n\
                  exceptions = [\"t\", \"i\", \"j\"]\n\
                  exception_patterns = [\"^[A-Z]$\"]",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
