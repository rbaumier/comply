//! drizzle-dollar-type-widens-unknown

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-dollar-type-widens-unknown",
    description: "`.$type<unknown>()` / `.$type<any>()` removes Drizzle's column type-safety with no benefit.",
    remediation: "Pass a concrete type to `.$type<...>()` (the JSON shape, the literal union, etc.).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
