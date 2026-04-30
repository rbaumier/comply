//! html-no-nested-interactive

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-nested-interactive",
    description: "Interactive elements must not be nested inside other interactive elements.",
    remediation: "Move the nested interactive element outside, or restructure the component.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
