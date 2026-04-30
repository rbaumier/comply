mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-query-callbacks",
    description: "`onSuccess`/`onError`/`onSettled` callbacks on `useQuery` were removed in v5.",
    remediation: "Move side-effects to `useEffect` watching the query result.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5"),
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
