//! rust-prefer-tracing-over-log — prefer `tracing` over the `log` crate.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-tracing-over-log",
    description: "Use the `tracing` crate instead of `log`.",
    remediation: "Replace `use log::…` and `log::{info,warn,error,debug,trace}!` \
                  with the equivalent `tracing` macros. `tracing` carries \
                  structured fields and span context across `async` boundaries, \
                  which `log` does not. The two crates can interoperate via \
                  `tracing-log` if you must support an external dependency \
                  that emits through `log`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "observability"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
