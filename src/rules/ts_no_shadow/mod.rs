//! ts-no-shadow — disallow variable declarations from shadowing outer scope variables.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-shadow",
    description: "Variable shadowing makes code harder to reason about and can lead to bugs.",
    remediation: "Rename the inner variable to avoid shadowing the outer one.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-shadow"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
