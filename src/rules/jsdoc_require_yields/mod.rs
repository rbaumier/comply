//! jsdoc/require-yields

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-yields",
    description: "Generator functions must document a @yields tag.",
    remediation: "Add `@yields {Type} description` to the JSDoc block of every generator function.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-yields.md",
    ),
    categories: &["jsdoc"],
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
