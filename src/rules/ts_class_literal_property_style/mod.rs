//! ts-class-literal-property-style — enforce consistent literal property style on classes.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-class-literal-property-style",
    description: "Enforce that literals on classes are exposed in a consistent style (fields vs getters).",
    remediation: "Use `readonly` fields for literals instead of trivial getter methods (default), or vice versa.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/class-literal-property-style/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
