//! ts-prefer-for-of — prefer `for-of` over index-only `for` loops.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-for-of",
    description: "A `for` loop whose index is only used for array access can be a simpler `for-of`.",
    remediation: "Replace the `for` loop with `for (const item of array)`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-for-of/"),
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
