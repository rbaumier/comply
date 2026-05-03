//! nuxt-plugin-no-sideeffect

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
