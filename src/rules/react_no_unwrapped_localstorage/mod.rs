//! react-no-unwrapped-localstorage — `localStorage.*` outside a `try`/`catch`.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
