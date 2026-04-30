//! elysia-better-auth-mount

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-better-auth-mount",
    description: "Better Auth handlers must be mounted, not used — `.use(auth.handler)` will not match Better Auth's nested route shape.",
    remediation: "Use `.mount(auth.handler)` (or `.mount('/api/auth', auth.handler)`) so the WHATWG handler receives the full request.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
