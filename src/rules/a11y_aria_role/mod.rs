//! a11y-aria-role

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-aria-role",
    description: "Flag invalid `role` attribute values in JSX.",
    remediation: "Use only valid WAI-ARIA role values. See the WAI-ARIA specification for the full list of valid roles.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
