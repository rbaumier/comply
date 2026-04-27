//! angular-no-any-in-template — `$any()` defeats template type checking.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-any-in-template",
    description: "`$any()` in templates disables type checking and hides errors.",
    remediation: "Type the field properly or use a getter / signal that returns the right shape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
