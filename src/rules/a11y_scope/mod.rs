//! a11y-scope

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-scope",
    description: "The `scope` attribute should only be used on `<th>` elements.",
    remediation: "Remove `scope` from non-`<th>` elements, or change the element to `<th>`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
