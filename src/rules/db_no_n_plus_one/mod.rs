//! db-no-n-plus-one

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "db-no-n-plus-one",
    description: "`await db.query` inside a loop is an N+1 query — use a JOIN or batch query.",
    remediation: "Move the query outside the loop: use a JOIN, `WHERE id IN (...)`, or batch fetch. N+1 queries scale linearly with result set size and are the #1 cause of slow pages.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
