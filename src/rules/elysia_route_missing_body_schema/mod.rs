//! elysia-route-missing-body-schema

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-route-missing-body-schema",
    description: "Elysia route handler reads `body` but the route has no `body:` schema.",
    remediation: "Declare `body: t.Object({...})` (or a registered model name) in the route options so Elysia validates and types the request body.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
