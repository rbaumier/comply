//! html-no-skip-heading-levels

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-skip-heading-levels",
    description: "Heading levels should not skip (e.g., h1 to h3 without h2).",
    remediation: "Use sequential heading levels for proper document outline.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
