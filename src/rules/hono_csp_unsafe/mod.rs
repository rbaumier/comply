//! hono-csp-unsafe

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-csp-unsafe",
    description: "`unsafe-inline` or `unsafe-eval` in CSP defeats its purpose.",
    remediation: "Use nonces (`NONCE` from `hono/secure-headers`) instead of `unsafe-inline`. Avoid `unsafe-eval` — it enables code injection.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
