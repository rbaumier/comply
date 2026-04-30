//! drizzle-multi-statement-tx

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-multi-statement-tx",
    description: "Sequential `db.insert`/`db.update`/`db.delete` in the same scope should run inside `db.transaction`.",
    remediation: "Wrap related mutating calls in `await db.transaction(async (tx) => { ... })` so partial failures roll back.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
