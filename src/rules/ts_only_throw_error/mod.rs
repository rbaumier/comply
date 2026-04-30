//! ts-only-throw-error — disallow throwing non-Error values.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-only-throw-error",
    description: "Only `Error` instances should be thrown — primitives and plain objects lose stack traces.",
    remediation: "Throw `new Error(...)` (or a subclass) rather than a string, number, or object literal.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/only-throw-error/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
