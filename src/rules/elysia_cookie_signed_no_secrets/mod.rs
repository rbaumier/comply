//! elysia-cookie-signed-no-secrets

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cookie-signed-no-secrets",
    description: "Cookie marked as signed but no `secrets` configured on the Elysia app.",
    remediation: "Configure `new Elysia({ cookie: { secrets: process.env.COOKIE_SECRETS! } })` so signed cookies can be verified.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
