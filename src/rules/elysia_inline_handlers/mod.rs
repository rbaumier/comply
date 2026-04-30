//! elysia-inline-handlers

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-inline-handlers",
    description: "Route handler passed by reference instead of inline — type inference is degraded.",
    remediation: "Use inline handlers for type inference: `.get('/', ({ body }) => Controller.method(body))`",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
