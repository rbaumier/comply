//! drizzle-created-at-default-now

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-created-at-default-now",
    description: "`createdAt` timestamp columns must have `.defaultNow()`.",
    remediation: "Chain `.defaultNow()` on `createdAt`/`created_at` timestamp columns so the database populates the value on insert.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
