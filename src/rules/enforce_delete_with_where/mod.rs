//! enforce-delete-with-where

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "enforce-delete-with-where",
    description: "`db.delete(table)` without a chained `.where(...)` deletes every row in the table.",
    remediation: "Add a `.where(condition)` clause, or use a dedicated truncate helper if you really mean to delete every row.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/sivaprasadreddy/eslint-plugin-drizzle#enforce-delete-with-where"),
    categories: &["database", "drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
