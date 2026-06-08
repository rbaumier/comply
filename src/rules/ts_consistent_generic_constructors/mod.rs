//! ts-consistent-generic-constructors — enforce generic type arguments on constructor call site.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-generic-constructors",
    description: "Generic type arguments should be on the constructor, not the variable annotation.",
    remediation: "Move the type argument from the type annotation to the constructor: `new Map<K, V>()` instead of `const m: Map<K, V> = new Map()`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-generic-constructors/"),
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
