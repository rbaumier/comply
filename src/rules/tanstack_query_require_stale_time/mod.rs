mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-require-stale-time",
    description: "`QueryClient` without a default `staleTime` refetches on every mount.",
    remediation: "Add `defaultOptions: { queries: { staleTime: 60_000 } }` to `QueryClient`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
