//! elysia-derive-validated-data

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-derive-validated-data",
    description: "`.derive()` callback reads `body`/`params`/`query` — those are pre-validation in `.derive()`.",
    remediation: "Use `.resolve(...)` instead; it runs after validation so `body`/`params`/`query` reflect the validated shape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
