//! ts-consistent-indexed-object-style — prefer `Record<K, V>` over index signatures.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-indexed-object-style",
    description: "Prefer `Record<K, V>` over manual index signature `{ [key: K]: V }` for consistency.",
    remediation: "Replace the index signature with `Record<K, V>`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-indexed-object-style/"),
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
