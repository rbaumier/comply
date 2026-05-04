//! ts-no-array-constructor — disallow generic `Array` constructors (TS extension).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-array-constructor",
    description: "Generic `Array` constructor is ambiguous — use array literal notation `[]`.",
    remediation: "Use `[]` or `Array.from()` instead. `Array<T>()` with type arguments is acceptable.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-array-constructor"),
    categories: &["typescript"],
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
