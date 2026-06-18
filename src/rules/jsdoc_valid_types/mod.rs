//! jsdoc/valid-types — imported from eslint-plugin-jsdoc.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/valid-types",
    description: "JSDoc `{...}` type expressions must be syntactically balanced and non-empty.",
    remediation: "Fix unbalanced braces/parens, remove empty `{}` types, and quote string-literal types that contain whitespace.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/valid-types.md",
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
