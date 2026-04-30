//! drizzle-zod-omit-generated

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-zod-omit-generated",
    description: "`createInsertSchema(table)` should `.omit({ id, createdAt, ... })` auto-generated columns.",
    remediation: "Chain `.omit({ id: true, createdAt: true, updatedAt: true })` on `createInsertSchema(...)` so API consumers don't submit DB-generated columns.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
