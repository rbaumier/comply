//! enforce-update-with-where

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "enforce-update-with-where",
    description: "`db.update(table).set(...)` without `.where(...)` updates every row in the table.",
    remediation: "Add a `.where(condition)` clause to bound the update.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/sivaprasadreddy/eslint-plugin-drizzle#enforce-update-with-where"),
    categories: &["database", "drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
