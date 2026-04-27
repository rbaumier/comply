//! nuxt-middleware-must-return

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-middleware-must-return",
    description: "Route middleware must return `navigateTo()`, `abortNavigation()`, or nothing.",
    remediation: "Replace bare values or thrown errors with `return navigateTo('/login')` / `return abortNavigation()`.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/guide/directory-structure/middleware"),
    categories: &["nuxt"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
