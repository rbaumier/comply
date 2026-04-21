//! rust-prefer-channel-over-arc-mutex-vec — mpsc beats shared Vec.
//!
//! `Arc<Mutex<Vec<_>>>` turns every concurrent `.push` into a
//! serialization point: threads queue on the mutex, completed work
//! sits behind the lock until the collector drains it, and the Vec's
//! capacity pressure amplifies the contention. An `mpsc::channel` is
//! the same pattern without the shared lock — writers send, the
//! collector iterates.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-channel-over-arc-mutex-vec",
    description: "`Arc<Mutex<Vec<` for collecting task results adds contention. Use `mpsc::channel` instead.",
    remediation: "Use `let (tx, rx) = mpsc::channel(); ... tx.send(result); let results: Vec<_> = rx.iter().collect();`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
