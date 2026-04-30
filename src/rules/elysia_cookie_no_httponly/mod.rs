//! elysia-cookie-no-httponly

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cookie-no-httponly",
    description: "Cookie config is missing `httpOnly: true` — cookie is readable from JavaScript (XSS vector).",
    remediation: "Add `httpOnly: true` to the cookie config (`t.Cookie({...})` or `cookie.set({...})`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
