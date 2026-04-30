//! drizzle-prefer-inarray

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-prefer-inarray",
    description: "Prefer `inArray(col, [...])` over `sql` templates with `IN (...)`.",
    remediation: "Replace `sql`… `IN (...)` with `inArray(col, [...])` — Drizzle's helper is parameterized and safer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
