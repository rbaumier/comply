//! drizzle-fk-needs-index

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-fk-needs-index",
    description: "Foreign key without an index — FK columns need explicit indexes.",
    remediation: "Add `.index()` on every FK column. PostgreSQL does NOT auto-index FK columns — without an index, cascading deletes and JOIN lookups do sequential scans.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
