//! drizzle-serverless-pool-max-one

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-serverless-pool-max-one",
    description: "In serverless (Edge/Lambda), `new Pool()` must set `max: 1`.",
    remediation: "Set `max: 1` in the `new Pool(...)` config in serverless code — each invocation has its own pool, and >1 multiplies DB connections linearly with concurrency.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
