//! rust-block-on-in-async — `block_on` from async = runtime panic.
//!
//! Calling `Runtime::block_on` (or `futures::executor::block_on`)
//! from within an async function asks tokio to start a new runtime
//! while one is already active. Tokio refuses with the famous
//! "Cannot start a runtime from within a runtime" panic.
//!
//! The fix is always `.await` — it's literally what you wanted in
//! the first place.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-block-on-in-async",
    description: "`block_on` from inside `async fn` panics the runtime.",
    remediation: "Replace `runtime.block_on(future)` with `future.await`. \
                  Calling `block_on` while a runtime is already running \
                  triggers tokio's `Cannot start a runtime from within a \
                  runtime` panic.",
    severity: Severity::Error,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
