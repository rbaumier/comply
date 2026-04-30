mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-invalidate-after-mutation",
    description: "A mutation performing a write request must invalidate or update the cache.",
    remediation: "Add `onSuccess` or `onSettled` that calls `queryClient.invalidateQueries(...)` or `queryClient.setQueryData(...)` so dependent queries refetch.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/invalidations-from-mutations"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
