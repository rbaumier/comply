mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-real-external-service",
    description: "Test makes a real network call to an external service — intercept it with MSW instead.",
    remediation: "Mock the external service with MSW (or equivalent) — never hit the real endpoint from tests.",
    severity: Severity::Error,
    doc_url: Some("https://mswjs.io/"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
