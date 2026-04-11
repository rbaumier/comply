//! a11y-no-static-element-interactions

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-static-element-interactions",
    description: "Flag `<div>` and `<span>` with `onClick` but no `role` attribute.",
    remediation: "Add a `role` attribute or use a native interactive element like `<button>` instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
