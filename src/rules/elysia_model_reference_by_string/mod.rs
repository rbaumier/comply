//! elysia-model-reference-by-string

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-model-reference-by-string",
    description: "Routes that import a TypeBox schema variable and use it inline lose Elysia's model registry deduplication.",
    remediation: "Register the schema with `.model({ name: schema })` once and reference it as `body: 'name'`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["maintainability", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
