//! ts-init-declarations — require initialization in variable declarations.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-init-declarations",
    description: "Variables should be initialized at declaration — uninitialized declarations are error-prone.",
    remediation: "Add an initializer to the variable declaration, or use `declare` for ambient contexts.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/init-declarations"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
