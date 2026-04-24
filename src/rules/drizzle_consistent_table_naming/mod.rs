//! drizzle-consistent-table-naming

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-consistent-table-naming",
    description: "Table names passed to `pgTable`/`mysqlTable`/`sqliteTable` should be lowercase snake_case plural.",
    remediation: "Rename the first string argument to a lowercase snake_case plural form (e.g. `user` → `users`, `orderItem` → `order_items`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
