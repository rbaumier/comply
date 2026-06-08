//! security-no-cors-reflect-origin — never reflect `Origin` without an allowlist.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-no-cors-reflect-origin",
    description: "Reflecting the request `Origin` header into `Access-Control-Allow-Origin` defeats CORS.",
    remediation: "Match the request `origin` against an explicit allowlist of trusted origins \
                  before echoing it back. Never reflect the raw header value.",
    severity: Severity::Error,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS"),
    categories: &["security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
