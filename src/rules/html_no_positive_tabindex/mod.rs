//! html-no-positive-tabindex
//!
//! Flags HTML `tabindex` attribute (lowercase) with a positive value.
//! Complements `a11y-tabindex-no-positive` which targets the JSX
//! `tabIndex` camelCase attribute.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-positive-tabindex",
    description: "HTML `tabindex` attribute must not be positive — it breaks natural tab order.",
    remediation: "Use `tabindex=\"0\"` (or `-1`) and rely on document order for focus sequence.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
