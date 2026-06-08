//! js-cache-repeated-storage — repeated `localStorage.getItem(key)` /
//! `sessionStorage.getItem(key)` with the same key in one function body.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "js-cache-repeated-storage",
    description: "Repeated `getItem()` calls with the same key — read once into a variable.",
    remediation: "Store the result of `localStorage.getItem(key)` in a variable and \
                  reuse it instead of calling `getItem` multiple times.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],

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
