//! elysia-deploy-prod-no-aot

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deploy-prod-no-aot",
    description: "`new Elysia({ ... })` configured without `aot: true` — production builds lose ahead-of-time compilation.",
    remediation: "Pass `aot: true` (or omit the flag if you intentionally want JIT) when constructing the Elysia instance used in production deployments.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
