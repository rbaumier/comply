//! a11y-heading-has-content

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-heading-has-content",
    description: "Headings (`h1`–`h6`) must have text content.",
    remediation: "Add text content inside the heading tag so screen readers can announce it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
