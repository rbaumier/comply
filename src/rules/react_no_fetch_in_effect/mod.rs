//! react-no-fetch-in-effect — `fetch()` inside `useEffect` is fragile (no
//! deduping, caching, retries, race protection). Prefer a data-fetching
//! library or a server component.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-fetch-in-effect",
    description: "`fetch()` inside `useEffect` lacks caching, deduping, and race protection.",
    remediation: "Use a data-fetching library (TanStack Query, SWR) or move \
                  fetching to a server component / loader.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
