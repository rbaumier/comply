//! drizzle-relations-missing-inverse

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-relations-missing-inverse",
    description: "A `relations(...)` block declares a `one(...)` / `many(...)` reference whose inverse isn't defined in the same file.",
    remediation: "Add the inverse `relations(...)` for the referenced table so Drizzle's relational query API resolves both directions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
