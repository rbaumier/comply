//! rust-no-sleep-in-test — `thread::sleep` / `tokio::time::sleep` in test code.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-sleep-in-test",
    description: "No `thread::sleep` or `tokio::time::sleep` inside tests.",
    remediation: "Sleep-based waits make tests slow and flaky. Wait on a \
                  condition (channel, signal, polling with a deadline) or \
                  inject a clock so the test can advance time deterministically. \
                  Tokio offers `tokio::time::pause()` + `advance(d)` for the \
                  latter.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "testing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
