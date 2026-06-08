//! prefer-number-properties — prefer `Number` static properties over globals.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-number-properties",
    description: "Prefer `Number.isNaN()`, `Number.parseInt()`, etc. over global equivalents.",
    remediation: "Replace global `isNaN()`, `isFinite()`, `parseInt()`, `parseFloat()`, `NaN`, \
                  and `Infinity` with their `Number.*` equivalents. The `Number` methods are \
                  stricter (no implicit coercion) and the properties are unambiguous.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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
