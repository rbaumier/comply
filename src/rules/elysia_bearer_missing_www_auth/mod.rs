//! elysia-bearer-missing-www-auth

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-bearer-missing-www-auth",
    description: "Bearer auth 401/400 response without `WWW-Authenticate` header — RFC 6750 violation.",
    remediation: "Add `set.headers['WWW-Authenticate'] = 'Bearer realm=\"...\"'` before returning the 401/400.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
