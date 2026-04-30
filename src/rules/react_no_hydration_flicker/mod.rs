//! react-no-hydration-flicker — `useEffect(() => setState(x), [])` on mount
//! causes a content flash during SSR hydration.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-hydration-flicker",
    description: "`useEffect(setState, [])` on mount causes a hydration flash.",
    remediation: "Use `useSyncExternalStore` with `getServerSnapshot` for SSR-safe \
                  external state, or add `suppressHydrationWarning` if the mismatch \
                  is intentional (e.g. timestamps, viewport size).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
