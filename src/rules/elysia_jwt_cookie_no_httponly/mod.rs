//! elysia-jwt-cookie-no-httponly

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-jwt-cookie-no-httponly",
    description: "Cookie storing a JWT is set without `httpOnly: true` — token is readable from JavaScript.",
    remediation: "Set `httpOnly: true` on cookies that store JWTs to prevent XSS theft.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
