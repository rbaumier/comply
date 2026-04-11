//! a11y-anchor-is-valid

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-anchor-is-valid",
    description: "Anchors must have a valid `href` — not `\"#\"`, `\"javascript:\"`, or missing.",
    remediation: "Use a real URL for `href`, or use a `<button>` if the element triggers an action.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
