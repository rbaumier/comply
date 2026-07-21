//! axum-serve-no-graceful-shutdown

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-serve-no-graceful-shutdown",
    description: "`axum::serve(...)` without `.with_graceful_shutdown(...)` — in-flight requests are dropped on SIGTERM/SIGINT.",
    remediation: "Chain `.with_graceful_shutdown(shutdown_signal())` onto \
                  `axum::serve(listener, app)` so open connections drain before \
                  the process exits.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["deployment", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
