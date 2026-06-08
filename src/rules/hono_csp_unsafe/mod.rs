//! hono-csp-unsafe

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-csp-unsafe",
    description: "`unsafe-inline` or `unsafe-eval` in CSP defeats its purpose.",
    remediation: "Use nonces (`NONCE` from `hono/secure-headers`) instead of `unsafe-inline`. Avoid `unsafe-eval` — it enables code injection.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
