//! drizzle-leftjoin-nullable-handling

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-leftjoin-nullable-handling",
    description: "`.leftJoin(...)` returns rows whose joined columns can be `null`, but the destructured result is consumed without a null check.",
    remediation: "Filter out rows whose joined entity is null, or treat the joined fields as nullable in the consumer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
