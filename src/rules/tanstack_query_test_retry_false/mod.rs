mod oxc_typescript;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-test-retry-false",
    description: "In test files, `new QueryClient` must disable retries to keep tests fast and deterministic.",
    remediation: "Set `defaultOptions: { queries: { retry: false } }` when instantiating `QueryClient` inside a test.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/testing"),
    categories: &["tanstack-query"],

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
