//! ts-explicit-function-return-type — require explicit return types on functions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-function-return-type",
    description: "Require explicit return types on functions and class methods.",
    remediation: "Add an explicit `: ReturnType` annotation after the parameter \
                  list. Explicit return types make function contracts visible \
                  and prevent silent drift when the implementation changes.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-function-return-type/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
