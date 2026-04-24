mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-enabled-on-suspense",
    description: "`useSuspenseQuery` does not support `enabled`.",
    remediation: "Conditionally render the component that calls `useSuspenseQuery` instead, or fall back to `useQuery` when you need to gate the request.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/reference/useSuspenseQuery"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
