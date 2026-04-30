//! elysia-objectstring-for-query

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-objectstring-for-query",
    description: "Nested `t.Object(...)` inside a `query:` schema — query string has no nested objects.",
    remediation: "Use `t.ObjectString({...})` for JSON-stringified objects passed via the query string.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
