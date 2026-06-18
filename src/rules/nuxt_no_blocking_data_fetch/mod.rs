//! nuxt-no-blocking-data-fetch

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-blocking-data-fetch",
    description: "Data fetching inside `defineNuxtRouteMiddleware` blocks navigation.",
    remediation: "Move the fetch into the page's `setup()` via `useFetch`/`useAsyncData`, or fall back to a server route.",
    severity: Severity::Warning,
    doc_url: Some("https://nuxt.com/docs/guide/directory-structure/middleware"),
    categories: &["nuxt", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
