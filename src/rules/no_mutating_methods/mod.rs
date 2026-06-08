//! no-mutating-methods

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-mutating-methods",
    description: "Disallow array mutating methods (push, pop, shift, unshift, splice, sort, reverse, fill, copyWithin).",
    remediation: "Use non-mutating alternatives: spread (`[...arr, x]`), `slice`, `toSorted`, `toReversed`, `toSpliced`, `filter`, `map`, or `concat`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],

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
