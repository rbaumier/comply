//! rust-unbounded-channel — every channel needs a capacity.
//!
//! `tokio::sync::mpsc::unbounded_channel()` and `std::sync::mpsc::channel()`
//! both return queues with no upper bound. If the producer is faster
//! than the consumer (and that's the entire point of using a channel),
//! memory grows without limit until the process is OOM-killed.
//!
//! The fix is always the same: pick a capacity. `mpsc::channel(N)`
//! gives you backpressure — producers `.await` when the queue is full
//! instead of silently piling up.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-unbounded-channel",
    description: "Unbounded channels can OOM the process.",
    remediation: "Use `mpsc::channel(N)` (tokio) or `crossbeam::channel::bounded(N)`. \
                  Pick a capacity that bounds memory under load — even \
                  N=1024 is infinitely safer than no bound. The producer \
                  will `.await` (or block) when full, providing backpressure.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
