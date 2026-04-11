//! rust-tokio-spawn-without-handle — fire-and-forget spawn swallows panics.
//!
//! `tokio::spawn(future)` returns a `JoinHandle`. If the caller drops
//! that handle (`tokio::spawn(...);` as a statement), every panic
//! inside the spawned task is silently swallowed — the runtime catches
//! it but no one observes it. The task can also be cancelled at
//! shutdown without anyone noticing.
//!
//! Either await the handle (so panics propagate), assign it to a
//! variable that's awaited later, or wrap in a logging helper that
//! converts the result into a structured error.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-tokio-spawn-without-handle",
    description: "`tokio::spawn(..)` whose JoinHandle is dropped silently swallows panics.",
    remediation: "Capture the JoinHandle and `.await` it, or pass the \
                  task through a wrapper like `tokio::spawn(async { \
                  if let Err(e) = work().await { tracing::error!(?e); } \
                  })`. Fire-and-forget loses every error and every panic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
