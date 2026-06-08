//! rust-thread-spawn-in-async — `std::thread::spawn` inside `async fn`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-thread-spawn-in-async",
    description: "`std::thread::spawn` invoked inside an `async fn`.",
    remediation: "Spawning an OS thread from async code defeats the runtime: \
                  the thread can't be cooperatively scheduled or driven by \
                  the executor, and CPU-bound work should go through \
                  `tokio::task::spawn_blocking` (or `rayon`). Use \
                  `tokio::spawn` for futures, `spawn_blocking` for sync work.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
