mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "testing-prefer-msw",
    description: "Mocking HTTP clients directly is brittle — use MSW to intercept at the network layer.",
    remediation: "Replace `vi.mock('axios')` / `global.fetch = vi.fn()` with an MSW request handler.",
    severity: Severity::Warning,
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
