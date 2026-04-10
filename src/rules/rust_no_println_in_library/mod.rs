//! rust-no-println-in-library — library code must use tracing, not println.
//!
//! Crate-type aware: skips files living in a pure binary crate (no
//! `src/lib.rs`). See `rust.rs` for the detection logic.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-println-in-library",
    description: "Library code must use tracing, not `println!` / `eprintln!`.",
    remediation: "Replace `println!` with `tracing::info!` / `tracing::debug!` \
                  and add structured fields. Library consumers configure the \
                  tracing subscriber; they cannot redirect `println!`. The rule \
                  auto-skips pure binary crates where writing to stdout is the \
                  whole point.",
    severity: Severity::Error,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
