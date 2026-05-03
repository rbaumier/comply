//! nuxt-middleware-must-return

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
