//! ts-no-unused-private-class-members — flag private class members that are never used.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unused-private-class-members",
    description: "Private class members that are never used are dead code.",
    remediation: "Remove the unused private member or use it.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unused-private-class-members"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
