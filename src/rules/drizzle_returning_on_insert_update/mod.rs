//! drizzle-returning-on-insert-update

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-returning-on-insert-update",
    description: "Drizzle insert/update without `.returning()` wastes a round-trip on a follow-up SELECT.",
    remediation: "Chain `.returning()` to get the inserted/updated row in a single query.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/insert#insert-returning"),
    categories: &["drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
