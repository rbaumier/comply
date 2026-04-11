//! ts-explicit-member-accessibility — require explicit accessibility modifiers on class members.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-member-accessibility",
    description: "Class properties and methods should have explicit accessibility modifiers.",
    remediation: "Add `public`, `private`, or `protected` to the class member declaration.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-member-accessibility/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
