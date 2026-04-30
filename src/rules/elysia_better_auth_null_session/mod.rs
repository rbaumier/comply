//! elysia-better-auth-null-session

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-better-auth-null-session",
    description: "Better Auth `auth.api.getSession` is called inside a macro `resolve` without a null-session check.",
    remediation: "Check `if (!session) return status(401)` before returning user/session — `getSession` returns null for unauthenticated requests.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
