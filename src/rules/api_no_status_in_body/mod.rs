//! api-no-status-in-body — flag response payloads that embed an HTTP
//! status code (`{ status: 200, data: ... }`). The transport already
//! carries the status; duplicating it leads to disagreement.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-status-in-body",
    description: "HTTP status codes belong in the response status, not in the body.",
    remediation: "Drop the `status` field. Set the HTTP response status instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
