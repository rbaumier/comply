//! drizzle-no-select-without-limit

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-select-without-limit",
    description: "`db.select().from(table)` without `.limit()` or `.where()` scans the entire table.",
    remediation: "Add `.limit(n)` or `.where(condition)` to bound the result set.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/select#basic-and-partial-select"),
    categories: &["drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
