//! elysia-better-auth-basepath

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-better-auth-basepath",
    description: "`betterAuth({ basePath: '' })` (or `'/'`) is invalid — Better Auth needs a real prefix.",
    remediation: "Set `basePath: '/api/auth'` (or any non-empty path other than `'/'`) when constructing `betterAuth`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
