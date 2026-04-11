//! hono-cookie-no-httponly

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-httponly",
    description: "Cookie set without `httpOnly` — accessible to JavaScript (XSS vector).",
    remediation: "Add `httpOnly: true` to cookie options: `setCookie(c, name, value, { httpOnly: true, secure: true, sameSite: 'Lax' })`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
