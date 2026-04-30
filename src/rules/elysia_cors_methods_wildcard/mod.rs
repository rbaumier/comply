//! elysia-cors-methods-wildcard

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cors-methods-wildcard",
    description: "`cors()` with `credentials: true` but no explicit `methods` allows every HTTP verb.",
    remediation: "Set `methods: ['GET', 'POST', ...]` explicitly when `credentials: true` so non-listed verbs are rejected.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
