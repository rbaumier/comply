//! ts-no-useless-constructor — disallow unnecessary constructors.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-useless-constructor",
    description: "Empty constructors that only call `super()` are unnecessary.",
    remediation: "Remove the constructor — the default behaviour is identical.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-useless-constructor/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
