//! drizzle-findfirst-without-where

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-findfirst-without-where",
    description: "`.findFirst()` without a `where:` clause returns an arbitrary row — almost always a bug.",
    remediation: "Pass `{ where: ... }` to scope the query, or use `.findFirst({ orderBy: ... })` if the row choice is intentional.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
