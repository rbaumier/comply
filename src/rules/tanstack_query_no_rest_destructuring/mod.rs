//! tanstack-query-no-rest-destructuring — `const { data, ...rest } = useQuery()`
//! subscribes to every field on the query result and re-renders on every state
//! transition.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-rest-destructuring",
    description: "Rest destructuring on a TanStack Query result subscribes to every field.",
    remediation: "Destructure only the fields you actually need (e.g. `data`, \
                  `isLoading`) instead of using `...rest`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
