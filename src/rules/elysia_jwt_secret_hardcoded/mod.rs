//! elysia-jwt-secret-hardcoded

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-jwt-secret-hardcoded",
    description: "JWT secret is a hardcoded string literal — leaks via source control.",
    remediation: "Read the secret from `process.env.JWT_SECRET` or a secret manager, never hardcode it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
