//! elysia-guard-overrides-route-schema

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-guard-overrides-route-schema",
    description: "Routes nested in a `.guard({ body: ... })` block redeclare `body:` — the inner schema overrides the guard.",
    remediation: "Drop the `body:` from the inner route or remove it from the guard so a single source of truth validates the request body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
