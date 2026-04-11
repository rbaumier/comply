//! a11y-control-has-associated-label

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-control-has-associated-label",
    description: "Interactive elements must have an accessible label.",
    remediation: "Add text content, `aria-label`, or `aria-labelledby` to `<button>`, `<input>`, `<select>`, and `<textarea>` elements. `<input type=\"hidden\">` is exempt.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
