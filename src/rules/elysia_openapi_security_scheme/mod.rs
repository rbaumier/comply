//! elysia-openapi-security-scheme

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-openapi-security-scheme",
    description: "Routes declare `security:` requirements without a matching `securitySchemes` definition — the generated OpenAPI doc is invalid.",
    remediation: "Define `securitySchemes` (e.g. `bearerAuth`) at the OpenAPI plugin level matching the route-level requirements.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
