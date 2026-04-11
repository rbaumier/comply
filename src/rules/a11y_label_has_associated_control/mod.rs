//! a11y-label-has-associated-control

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-label-has-associated-control",
    description: "`<label>` must have an associated control via `htmlFor` or by wrapping an input.",
    remediation: "Add `htmlFor=\"input-id\"` to the `<label>` or wrap an `<input>`, `<select>`, or `<textarea>` inside it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
