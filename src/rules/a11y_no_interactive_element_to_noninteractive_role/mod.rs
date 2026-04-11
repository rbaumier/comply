//! a11y-no-interactive-element-to-noninteractive-role

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-interactive-element-to-noninteractive-role",
    description: "Interactive elements must not be assigned non-interactive ARIA roles.",
    remediation: "Remove the non-interactive `role` or use a non-interactive element instead of `<button>`, `<a>`, `<input>`, `<select>`, or `<textarea>`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
