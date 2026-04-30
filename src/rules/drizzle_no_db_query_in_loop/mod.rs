//! drizzle-no-db-query-in-loop

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-db-query-in-loop",
    description: "Drizzle queries inside `for` / `for-of` / `.map` / `.forEach` cause N+1 round-trips to the database.",
    remediation: "Hoist the query out of the loop and use `inArray(...)`/`leftJoin(...)`/`with: {...}` to fetch in a single round-trip.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
