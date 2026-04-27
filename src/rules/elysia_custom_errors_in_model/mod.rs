//! elysia-custom-errors-in-model

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-custom-errors-in-model",
    description: "Custom error class declared in a service file — Elysia error mapping lives next to the model.",
    remediation: "Move `class FooError extends Error` to the matching `*.model.ts` so `app.error({ FOO: FooError })` and the schema stay co-located.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["architecture", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
