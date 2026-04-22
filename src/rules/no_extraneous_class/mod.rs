//! no-extraneous-class — disallow classes used only as static namespaces
//! or with no members at all. Ports typescript-eslint's
//! `@typescript-eslint/no-extraneous-class`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-extraneous-class",
    description: "Class has no instance state and is only used as a namespace or is empty.",
    remediation: "Replace the class with plain module-level `export`s, a frozen object literal, \
                  or remove it entirely. Classes that only hold static members add ceremony \
                  without value.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-extraneous-class"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
