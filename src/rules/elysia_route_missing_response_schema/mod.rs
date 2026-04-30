//! elysia-route-missing-response-schema

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-route-missing-response-schema",
    description: "Elysia route validates input but has no `response:` schema.",
    remediation: "Add `response: { 200: t.Object({...}) }` so the OpenAPI doc and Eden client know the success shape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
