//! rust-no-allow-without-reason — `#[allow(...)]` without a justification comment.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-allow-without-reason",
    description: "`#[allow(...)]` without a justification comment hides problems silently.",
    remediation: "Add a `//` comment on the same line or the line above explaining \
                  why the lint is suppressed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
