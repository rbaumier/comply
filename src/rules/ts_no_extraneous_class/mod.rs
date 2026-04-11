//! ts-no-extraneous-class — disallow classes used as namespaces.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-extraneous-class",
    description: "Classes with only static members or an empty body should be plain objects or modules.",
    remediation: "Use a module/namespace, plain object, or standalone functions instead.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-extraneous-class/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
