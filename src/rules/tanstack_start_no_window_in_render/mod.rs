//! tanstack-start-no-window-in-render — forbid `window.*`/`document.*` in the
//! render body of route components (breaks SSR).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-window-in-render",
    description: "`window.*` / `document.*` in render breaks SSR.",
    remediation: "Read from `window`/`document` inside a `useEffect` or behind \
                  a `typeof window !== 'undefined'` guard.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start", "react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
