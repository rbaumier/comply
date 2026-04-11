//! a11y-no-redundant-roles

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-redundant-roles",
    description: "Flag elements with explicit roles matching their implicit ARIA role.",
    remediation: "Remove the redundant `role` attribute. The element already has this role implicitly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
