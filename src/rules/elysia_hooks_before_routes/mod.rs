//! elysia-hooks-before-routes

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-hooks-before-routes",
    description: "Lifecycle hook chained after route definitions — Elysia hooks only apply to routes registered after them.",
    remediation: "Chain `.onBeforeHandle(...)`, `.onError(...)`, etc. before `.get(...)`/`.post(...)` so they apply to subsequent routes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
