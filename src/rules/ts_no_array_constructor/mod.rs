//! ts-no-array-constructor — disallow generic `Array` constructors (TS extension).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-array-constructor",
    description: "Generic `Array` constructor is ambiguous — use array literal notation `[]`.",
    remediation: "Use `[]` or `Array.from()` instead. `Array<T>()` with type arguments is acceptable.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-array-constructor"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
