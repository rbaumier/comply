//! elysia-response-keyed-by-status

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-response-keyed-by-status",
    description: "`response:` is a single TypeBox schema instead of being keyed by HTTP status.",
    remediation: "Use `response: { 200: t.Object({...}), 404: t.Object({...}) }` so error variants are typed alongside the success body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
