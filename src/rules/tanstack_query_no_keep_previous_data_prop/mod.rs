mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-keep-previous-data-prop",
    description: "`keepPreviousData: true` was replaced by `placeholderData: keepPreviousData` in v5.",
    remediation: "Import `keepPreviousData` from `@tanstack/react-query` and use `placeholderData: keepPreviousData`.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5"),
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
