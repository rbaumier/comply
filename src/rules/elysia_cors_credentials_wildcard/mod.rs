//! elysia-cors-credentials-wildcard

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cors-credentials-wildcard",
    description: "CORS `credentials: true` requires an explicit origin — wildcard is rejected by browsers.",
    remediation: "Set a specific `origin: 'https://your-domain.com'` whenever `credentials: true` is enabled.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
