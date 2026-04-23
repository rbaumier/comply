//! pg-require-limit

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "pg-require-limit",
    description: "SQL `SELECT` statements without a `LIMIT` clause can return unbounded rows.",
    remediation: "Add a `LIMIT n` clause, a `COUNT(..)` / aggregate, or a unique `WHERE` predicate (e.g. `WHERE id = ...`) so the query is bounded.",
    severity: Severity::Error,
    doc_url: Some("https://wiki.postgresql.org/wiki/Don%27t_Do_This#Don.27t_forget_LIMIT"),
    categories: &["database", "postgresql"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
