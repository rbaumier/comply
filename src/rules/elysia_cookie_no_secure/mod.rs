//! elysia-cookie-no-secure

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cookie-no-secure",
    description: "Cookie config is missing `secure: true` — cookie can be sent over plain HTTP.",
    remediation: "Add `secure: true` so the cookie is only sent over HTTPS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
