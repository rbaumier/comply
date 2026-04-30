//! html-require-input-label

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-input-label",
    description: "Form inputs must have accessible labels.",
    remediation: "Add a <label> element with htmlFor, wrap in label, or use aria-label/aria-labelledby.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
