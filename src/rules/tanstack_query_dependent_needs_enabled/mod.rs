mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-dependent-needs-enabled",
    description: "Dependent queries (queryFn reads a possibly-undefined value) must set `enabled`.",
    remediation: "Add `enabled: !!value` (or a more precise guard) so the query doesn't fire until the dependency is defined.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/dependent-queries"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
