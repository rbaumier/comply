//! elysia-cf-no-inline-values

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cf-no-inline-values",
    description: "Inline string handler under Cloudflare adapter — string handlers bypass the proper compiled path on Workers.",
    remediation: "Use a function handler: `.get('/', () => 'Hello')` instead of `.get('/', 'Hello')`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
