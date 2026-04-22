//! jsdoc/require-throws

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-throws",
    description: "Functions that throw must document a @throws tag.",
    remediation: "Add `@throws {ErrorType} when ...` to describe the failure case.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-throws.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    let backends = vec![
        (Language::TypeScript, Backend::Text(Box::new(text::Check))),
        (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        (Language::Tsx, Backend::Text(Box::new(text::Check))),
        (Language::Rust, Backend::Text(Box::new(text::Check))),
    ];
    RuleDef { meta: META, backends }
}
