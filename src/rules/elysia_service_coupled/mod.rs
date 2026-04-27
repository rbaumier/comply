//! elysia-service-coupled

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-service-coupled",
    description: "Service module imports framework symbols from `elysia` — couples the service layer to the HTTP layer.",
    remediation: "Keep services framework-agnostic: throw plain errors and let route handlers translate them. Only `status` is allowed to cross from `elysia` for ergonomic HTTP errors.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["architecture", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
