//! rust-must-use-on-result — public Result-returning fns need `#[must_use]`.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-must-use-on-result",
    description: "Public functions returning Result/Option need `#[must_use]`.",
    remediation: "Add `#[must_use]` above the function signature. Without \
                  it, callers can silently discard the Result and lose every \
                  error. Enable `clippy::must_use_candidate` in your crate.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
