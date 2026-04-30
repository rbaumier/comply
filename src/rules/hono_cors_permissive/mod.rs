//! hono-cors-permissive

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-cors-permissive",
    description: "Permissive CORS allows any origin to access the API.",
    remediation: "Restrict `cors({ origin: 'https://your-domain.com' })`. Default `cors()` sets `origin: '*'`. With `credentials: true`, the origin must be explicit.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
