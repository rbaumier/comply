//! tanstack-start-no-date-now-in-render — forbid `Date.now()`, `new Date()`,
//! `Math.random()` in the render body of route components (hydration mismatch).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-date-now-in-render",
    description: "`Date.now()`, `new Date()`, `Math.random()` in render cause \
                  hydration mismatches.",
    remediation: "Compute non-deterministic values inside a `useEffect`, a \
                  loader, or a server function.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start", "react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
