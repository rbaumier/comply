//! no-document-domain

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-document-domain",
    description: "Do not assign to `document.domain`.",
    remediation: "Avoid writing to `document.domain` — it relaxes the same-origin policy and creates a security hole. Use `postMessage` or CORS-based communication between origins instead.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
