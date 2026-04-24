mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-global-onerror-v5",
    description: "`defaultOptions.queries.onError` was removed in v5 — use `QueryCache({ onError })`.",
    remediation: "Move the global error handler to the QueryCache: `new QueryClient({ queryCache: new QueryCache({ onError }) })`.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5#callbacks-on-usequery-have-been-removed"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
