//! react-no-unwrapped-localstorage — `localStorage.*` outside a `try`/`catch`.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-unwrapped-localstorage",
    description: "`localStorage.getItem`/`setItem` throws in private-browsing mode, quota \
                  exhaustion, and server-side rendering. Calling it unwrapped crashes the app.",
    remediation: "Wrap `localStorage` access in `try { ... } catch (e) { ... }` and \
                  provide a safe fallback.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
