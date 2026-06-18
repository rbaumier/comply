//! testing-no-mocking-internal-modules — flag `vi.mock('./...')` / `jest.mock('./...')`
//! of relative internal paths. Tests should mock boundaries, not internals.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-mocking-internal-modules",
    description: "Mocking a relative internal module couples tests to implementation details.",
    remediation: "Mock only external boundaries (HTTP, DB, third-party SDKs). Refactor so the collaborator is injected, or rely on the real internal module.",
    severity: Severity::Warning,
    doc_url: None,
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
