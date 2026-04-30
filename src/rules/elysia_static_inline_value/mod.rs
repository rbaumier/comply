//! elysia-static-inline-value

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-static-inline-value",
    description: "Route handler returns only a static string literal — pass the literal directly for ahead-of-time response caching.",
    remediation: "Replace `.get('/health', () => 'ok')` with `.get('/health', 'ok')`. Elysia compiles literal responses ahead of time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
