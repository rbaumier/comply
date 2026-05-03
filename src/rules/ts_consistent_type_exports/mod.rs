//! ts-consistent-type-exports — require `export type` for type-only re-exports.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-type-exports",
    description: "Type-only exports should use `export type` rather than `export`.",
    remediation: "Replace `export { Foo }` with `export type { Foo }` when only types are re-exported.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-type-exports/"),
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
