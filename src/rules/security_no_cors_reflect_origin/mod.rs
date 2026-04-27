//! security-no-cors-reflect-origin — never reflect `Origin` without an allowlist.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "security-no-cors-reflect-origin",
    description: "Reflecting the request `Origin` header into `Access-Control-Allow-Origin` defeats CORS.",
    remediation: "Match the request `origin` against an explicit allowlist of trusted origins \
                  before echoing it back. Never reflect the raw header value.",
    severity: Severity::Error,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS"),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
