//! a11y-anchor-has-content

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-anchor-has-content",
    description: "Anchors must have text content for screen readers.",
    remediation: "Add text content inside the `<a>` tag, or use `aria-label` for accessible labeling.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
