//! nuxt-no-global-state-in-composable

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-global-state-in-composable",
    description: "Module-level `let`/`var` in a composable leaks state across SSR requests.",
    remediation: "Move the state inside the composable function body, or use `useState()` to bind it to the request lifecycle.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/getting-started/state-management"),
    categories: &["nuxt", "ssr"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
