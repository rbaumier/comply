mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-test-retry-false",
    description: "In test files, `new QueryClient` must disable retries to keep tests fast and deterministic.",
    remediation: "Set `defaultOptions: { queries: { retry: false } }` when instantiating `QueryClient` inside a test.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/testing"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
