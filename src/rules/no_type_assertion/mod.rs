//! Bans all `as T` type assertions — prefer type guards, generics, or `satisfies`.
//!
//! Type assertions bypass the type checker. Even "safe" assertions can mask
//! bugs when the underlying data changes. Use `satisfies` for type checking
//! without widening, or refactor to use proper type guards.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-type-assertion",
    description: "Bans all `as T` type assertions.",
    remediation: "Use `satisfies T` for validation, type guards for narrowing, or generics for polymorphism.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
