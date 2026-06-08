//! Bans all `as T` type assertions — prefer type guards, generics, or `satisfies`.
//!
//! Type assertions bypass the type checker. Even "safe" assertions can mask
//! bugs when the underlying data changes. Use `satisfies` for type checking
//! without widening, or refactor to use proper type guards.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-type-assertion",
    description: "Bans all `as T` type assertions.",
    remediation: "Use `satisfies T` for validation, type guards for narrowing, or generics for polymorphism.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],

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
