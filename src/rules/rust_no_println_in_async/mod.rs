//! rust-no-println-in-async — `println!` / `eprintln!` inside async
//! code takes a blocking stdout/stderr lock. On an async runtime that
//! stalls the reactor thread, corrupts interleaved output from
//! concurrent tasks, and bypasses the structured logging pipeline.
//!
//! Use a `tracing` macro (`info!`, `warn!`, `debug!`, …) which
//! integrates with the runtime's logging infrastructure and respects
//! subscriber filters / spans.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-println-in-async",
    description: "`println!` / `eprintln!` inside async code blocks the runtime.",
    remediation: "Replace `println!(\"…\")` / `eprintln!(\"…\")` with a `tracing` \
                  macro (`tracing::info!`, `tracing::warn!`, `tracing::error!`). \
                  `println!` takes a blocking stdout lock — inside an async task \
                  that stalls the reactor and interleaves output from concurrent \
                  tasks. Tracing macros are non-blocking and respect subscriber \
                  filters/spans.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
