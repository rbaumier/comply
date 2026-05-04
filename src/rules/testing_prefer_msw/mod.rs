//! testing-prefer-msw — flag direct HTTP-client mocking in tests.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;
use crate::rules::backend::Backend;

pub const META: RuleMeta = RuleMeta {
    id: "testing-prefer-msw",
    description: "Mocking HTTP clients directly is brittle — use MSW to intercept at the network layer.",
    remediation: "Replace `vi.mock('axios')` / `jest.mock('node-fetch')` / `global.fetch = vi.fn()` with an MSW request handler.",
    severity: Severity::Warning,
    doc_url: Some("https://mswjs.io/"),
    categories: &["testing"],
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
