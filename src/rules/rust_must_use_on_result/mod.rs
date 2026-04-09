//! rust-must-use-on-result — public Result-returning fns need `#[must_use]`.

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-must-use-on-result",
    description: "Public functions returning `Result` need `#[must_use]`.",
    remediation: "Add `#[must_use]` above the function signature. Without \
                  it, callers can silently discard the Result and lose every \
                  error — the exact outcome `Result` exists to prevent. \
                  Trait impl methods are exempted (visibility is inherited).",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
