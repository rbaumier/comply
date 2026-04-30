//! no-ignored-exceptions

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-ignored-exceptions",
    description: "Empty `catch` block silently swallows exceptions.",
    remediation: "At minimum, log the error or re-throw it. Silent catch blocks hide bugs and make debugging extremely difficult. If intentional, add an explanatory comment.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
