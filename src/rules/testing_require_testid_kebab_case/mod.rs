//! testing-require-testid-kebab-case — enforce kebab-case for `data-testid`
//! / `data-test` attribute values.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-require-testid-kebab-case",
    description: "data-testid / data-test values must be kebab-case for consistent, selector-safe querying.",
    remediation: "Use lowercase letters, digits, and hyphens only (e.g. 'submit-button', 'user-card-name').",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
