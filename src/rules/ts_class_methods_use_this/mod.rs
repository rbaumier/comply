//! ts-class-methods-use-this — enforce that class methods utilize `this`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-class-methods-use-this",
    description: "Class methods that don't use `this` should be static or extracted to a standalone function.",
    remediation: "Add `static` to the method, move it to a standalone function, or use `this` in the body.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/class-methods-use-this"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
