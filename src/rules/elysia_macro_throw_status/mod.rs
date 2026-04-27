//! elysia-macro-throw-status

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-macro-throw-status",
    description: "Code uses `throw status(...)` — Elysia macros and resolvers expect `return status(...)`.",
    remediation: "Replace `throw status(...)` with `return status(...)` so Elysia tracks the response type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
