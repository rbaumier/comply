//! rust-no-unwrap — no `.unwrap()` or `.expect()` in production code.
//!
//! Delegated entirely to clippy. This module exists so the rule shows
//! up in `comply list` / `comply explain` — the actual enforcement
//! runs via `cargo clippy`. See rust.rs for setup details.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-unwrap",
    description: "No `.unwrap()` / `.expect()` in production code.",
    remediation: "Handle the None / Err case explicitly. Use `?` with \
                  proper error propagation, or `unwrap_or_else` with a \
                  meaningful fallback. `unwrap()` turns runtime conditions \
                  into crashes. Enable `clippy::unwrap_used` and \
                  `clippy::expect_used` in your crate root to enforce.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
