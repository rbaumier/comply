//! tanstack-query-stable-client — `new QueryClient()` inside a component
//! creates a fresh cache on every render.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-stable-client",
    description: "`new QueryClient()` inside a component recreates the cache every render.",
    remediation: "Hoist `new QueryClient()` to module scope, or wrap it in \
                  `useState(() => new QueryClient())` / `useRef`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-query"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
