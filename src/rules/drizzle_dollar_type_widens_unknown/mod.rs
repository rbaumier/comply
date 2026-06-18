//! drizzle-dollar-type-widens-unknown

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-dollar-type-widens-unknown",
    description: "`.$type<unknown>()` / `.$type<any>()` removes Drizzle's column type-safety with no benefit.",
    remediation: "Pass a concrete type to `.$type<...>()` (the JSON shape, the literal union, etc.).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle"],

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
