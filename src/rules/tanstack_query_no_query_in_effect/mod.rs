//! tanstack-query-no-query-in-effect — `useQuery()` already has its own
//! lifecycle; calling it from a `useEffect` defeats the point.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-query-in-effect",
    description: "TanStack Query hook called inside `useEffect`.",
    remediation: "Call `useQuery` at the top level of the component — it \
                  manages its own subscriptions, refetching, and cleanup.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
