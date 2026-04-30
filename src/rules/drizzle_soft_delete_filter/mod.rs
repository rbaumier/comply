//! drizzle-soft-delete-filter

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-soft-delete-filter",
    description: "Queries on soft-deletable tables must filter `isNull(t.deletedAt)`.",
    remediation: "Add `isNull(t.deletedAt)` (or equivalent) in the `where` clause of `select()` / `findMany()` calls in modules that reference `deletedAt`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
