//! elysia-cors-wildcard

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cors-wildcard",
    description: "Permissive CORS allows any origin to access the Elysia API.",
    remediation: "Restrict the origin: `cors({ origin: 'https://your-domain.com' })`. Default `cors()` allows all origins.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
