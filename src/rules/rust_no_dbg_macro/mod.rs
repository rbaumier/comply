//! rust-no-dbg-macro — `dbg!()` is for debugging, never for production.
//!
//! `dbg!()` was added to std specifically as a temporary debugging
//! aid: print the file/line and the value, then return the value.
//! It pollutes stderr, slows the hot path, and can leak sensitive
//! data into logs. The whole point of having it as a dedicated
//! macro instead of `println!` is that it's grep-able for cleanup.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-dbg-macro",
    description: "`dbg!()` is a debugging aid that must not ship.",
    remediation: "Remove the `dbg!()` call. If you need permanent \
                  observability, use `tracing::debug!`/`tracing::info!` \
                  with structured fields instead. `dbg!()` writes to \
                  stderr unconditionally and can leak PII.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
