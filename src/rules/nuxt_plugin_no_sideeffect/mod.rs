//! nuxt-plugin-no-sideeffect

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-plugin-no-sideeffect",
    description: "Nuxt plugins must wrap their logic in `defineNuxtPlugin` so they receive the Nuxt app instance.",
    remediation: "Move top-level statements into the `defineNuxtPlugin((nuxtApp) => { ... })` callback.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/guide/directory-structure/plugins"),
    categories: &["nuxt"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
