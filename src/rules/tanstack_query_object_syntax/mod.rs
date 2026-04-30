mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-object-syntax",
    description: "TanStack Query v5 requires object syntax: `useQuery({ queryKey, queryFn })`.",
    remediation: "Replace the positional form `useQuery(key, fn, opts)` with `useQuery({ queryKey: key, queryFn: fn, ...opts })`.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
