//! rust-thread-sleep-in-async — `std::thread::sleep` blocks the runtime.
//!
//! Calling `std::thread::sleep(d)` from inside an `async fn` parks
//! the OS thread for the requested duration. With tokio's default
//! multi-thread runtime, that worker thread can no longer poll any
//! other future — every concurrent task gets stuck behind this one
//! sleep. The whole point of async is to release the thread on
//! waits; `tokio::time::sleep(d).await` does that correctly.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-thread-sleep-in-async",
    description: "`std::thread::sleep` from `async fn` blocks the runtime.",
    remediation: "Replace `std::thread::sleep(d)` with `tokio::time::sleep(d).await` \
                  (or your runtime's equivalent). The async version yields the \
                  worker thread back to the runtime instead of parking it.",
    severity: Severity::Error,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
