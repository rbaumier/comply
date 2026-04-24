//! drizzle-camel-snake-column-names

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-camel-snake-column-names",
    description: "TS property should be camelCase while the column string argument should be snake_case.",
    remediation: "Keep the TS property name camelCase and pass the snake_case database column name as the first string argument to `varchar`/`text`/`integer`/etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
