//! elysia-jwt-missing-exp

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-jwt-missing-exp",
    description: "JWT plugin configured without `exp` — tokens never expire.",
    remediation: "Add `exp: '7d'` (or another duration) to the `jwt({ ... })` config so tokens have an expiry.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
