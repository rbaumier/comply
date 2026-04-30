//! elysia-service-return-not-throw

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-service-return-not-throw",
    description: "`throw` in an Elysia service breaks typed error propagation — return `status(...)` instead.",
    remediation: "Return `status(code, message)` instead of throwing — Elysia convention for typed error propagation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
