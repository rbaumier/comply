//! a11y-no-access-key

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-access-key",
    description: "Avoid using `accessKey` — it conflicts with screen reader keyboard shortcuts.",
    remediation: "Remove the `accessKey` attribute. Provide alternative navigation methods instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
