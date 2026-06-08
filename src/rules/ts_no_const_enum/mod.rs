//! ts-no-const-enum — flag `const enum` declarations.
//!
//! `const enum` is inlined at compile time, which breaks with `isolatedModules`,
//! produces surprising emit behavior across bundlers, and loses the declaration
//! at runtime. A regular enum or a literal union type is safer.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-const-enum",
    description: "`const enum` declarations are inlined and incompatible with isolatedModules.",
    remediation: "Use regular enum or union types instead of const enum",
    severity: Severity::Warning,
    doc_url: Some("https://www.typescriptlang.org/docs/handbook/enums.html#const-enums"),
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
