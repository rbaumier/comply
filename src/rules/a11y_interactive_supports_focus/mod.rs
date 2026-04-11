//! a11y-interactive-supports-focus

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-interactive-supports-focus",
    description: "Elements with interactive handlers and an interactive role must be focusable.",
    remediation: "Add `tabIndex={0}` to elements that have `onClick`/`onKeyDown` and an interactive `role`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
