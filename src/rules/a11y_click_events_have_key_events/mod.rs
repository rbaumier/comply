//! a11y-click-events-have-key-events

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-click-events-have-key-events",
    description: "Elements with `onClick` must also have a keyboard event handler.",
    remediation: "Add `onKeyDown`, `onKeyUp`, or `onKeyPress` alongside `onClick` for keyboard accessibility.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
