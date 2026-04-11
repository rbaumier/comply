//! a11y-no-autofocus

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-autofocus",
    description: "Avoid using `autoFocus` — it is disorienting for screen reader users.",
    remediation: "Remove `autoFocus` and let the user navigate to the element naturally.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
