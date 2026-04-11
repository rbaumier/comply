//! ts-explicit-function-return-type — require explicit return types on functions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-function-return-type",
    description: "Functions and class methods should have explicit return types for documentation and safety.",
    remediation: "Add an explicit return type annotation to the function.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-function-return-type/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
