//! elysia-cookie-removal-api

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cookie-removal-api",
    description: "Setting `cookie.x.value = ''` doesn't clear the cookie — it sends an empty value with the same expiry.",
    remediation: "Use `cookie.x.remove()` (or `delete cookie.x`) so Elysia emits a Set-Cookie with an expired Max-Age.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
