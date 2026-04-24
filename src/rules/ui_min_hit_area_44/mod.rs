//! ui-min-hit-area-44 — interactive elements should be ≥ 44×44 CSS px.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-min-hit-area-44",
    description: "Interactive elements (button, a, input) should have a tap target of at least 44×44 CSS pixels.",
    remediation: "Avoid utility classes that force a small size (h-3/w-3/h-4/w-4); pad the element so its hit area is ≥ 44px.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui", "a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
