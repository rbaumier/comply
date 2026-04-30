//! elysia-server-timing-prod

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-server-timing-prod",
    description: "`serverTiming({ enabled: true })` hardcodes the header on — exposing internal timings to every client.",
    remediation: "Tie `enabled` to `process.env.NODE_ENV !== 'production'` or another internal-only flag.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
