//! elysia-openapi-from-types-prod

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-openapi-from-types-prod",
    description: "`fromTypes('src/index.ts')` reads source files at runtime — should be conditional for prod builds.",
    remediation: "Gate `fromTypes()` behind `process.env.NODE_ENV !== 'production'` or pre-compute the spec at build time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
