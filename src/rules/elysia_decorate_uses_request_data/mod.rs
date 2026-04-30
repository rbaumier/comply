//! elysia-decorate-uses-request-data

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-decorate-uses-request-data",
    description: "`.decorate(...)` runs at boot — calling `Date.now()` or `Math.random()` there freezes a value across all requests.",
    remediation: "Move per-request values to `.derive(...)` so they are computed for each request.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
