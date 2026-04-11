//! a11y-mouse-events-have-key-events

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-mouse-events-have-key-events",
    description: "Flag `onMouseOver` without `onFocus` and `onMouseOut` without `onBlur`.",
    remediation: "Add `onFocus` alongside `onMouseOver` and `onBlur` alongside `onMouseOut` to ensure keyboard accessibility.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
