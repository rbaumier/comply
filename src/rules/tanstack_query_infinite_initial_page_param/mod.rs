mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-infinite-initial-page-param",
    description: "`useInfiniteQuery` and `infiniteQueryOptions` require `initialPageParam` in v5.",
    remediation: "Add `initialPageParam` to the options object: `useInfiniteQuery({ queryKey, queryFn, initialPageParam: 0, getNextPageParam })`.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5#infinite-queries"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
