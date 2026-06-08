//! jsdoc-reject-any-type

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-reject-any-type",
    description: "JSDoc uses `*` or `any` as a type, which defeats the purpose of type documentation.",
    remediation: "Replace the `*`/`any` type annotation with a specific type.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/no-undefined-types.md",
    ),
    categories: &["jsdoc"],

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
