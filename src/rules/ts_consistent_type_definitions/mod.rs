//! ts-consistent-type-definitions — enforce `interface` vs `type` for object type definitions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-type-definitions",
    description: "Enforce consistent use of `interface` or `type` for object type definitions.",
    remediation: "Use `interface` for object shapes (default), or use `type` consistently.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-type-definitions/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
