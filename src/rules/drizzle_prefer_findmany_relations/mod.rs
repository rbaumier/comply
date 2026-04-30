//! drizzle-prefer-findmany-relations

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-prefer-findmany-relations",
    description: "Prefer `db.query.X.findMany({ with })` over manual `.leftJoin` / `.innerJoin` chains when relations are defined.",
    remediation: "Use the relational query API (`db.query.X.findMany({ with: { ... } })`) instead of assembling the result manually with `.leftJoin` / `.innerJoin`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
