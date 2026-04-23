//! ts-no-const-enum — flag `const enum` declarations.
//!
//! `const enum` is inlined at compile time, which breaks with `isolatedModules`,
//! produces surprising emit behavior across bundlers, and loses the declaration
//! at runtime. A regular enum or a literal union type is safer.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-const-enum",
    description: "`const enum` declarations are inlined and incompatible with isolatedModules.",
    remediation: "Use regular enum or union types instead of const enum",
    severity: Severity::Warning,
    doc_url: Some("https://www.typescriptlang.org/docs/handbook/enums.html#const-enums"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
