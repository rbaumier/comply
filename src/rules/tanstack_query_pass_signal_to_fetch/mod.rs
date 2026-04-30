mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-pass-signal-to-fetch",
    description: "A `queryFn` that destructures `{ signal }` should forward it to `fetch` for cancellation.",
    remediation: "Pass the signal through: `fetch(url, { signal })`. Otherwise in-flight requests won't be aborted when the query is cancelled.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/query-cancellation"),
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
