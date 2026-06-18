//! testing-no-real-external-service — flag `fetch`/`axios` calls to real external URLs in tests.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-real-external-service",
    description: "Test makes a real network call to an external service — intercept it with MSW instead.",
    remediation: "Mock the external service with MSW (or equivalent) — never hit the real endpoint from tests.",
    severity: Severity::Error,
    doc_url: Some("https://mswjs.io/"),
    categories: &["testing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
