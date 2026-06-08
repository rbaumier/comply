//! nuxt-no-global-state-in-composable

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-global-state-in-composable",
    description: "Module-level `let`/`var` in a composable leaks state across SSR requests.",
    remediation: "Move the state inside the composable function body, or use `useState()` to bind it to the request lifecycle.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/getting-started/state-management"),
    categories: &["nuxt", "ssr"],

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
