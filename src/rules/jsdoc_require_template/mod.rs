//! jsdoc/require-template

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-template",
    description: "Generic functions must document each type parameter with @template.",
    remediation: "Add `@template T` (or `@template {Constraint} T`) for each generic type parameter.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-template.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    let backends = vec![
        (Language::TypeScript, Backend::Text(Box::new(text::Check))),
        (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        (Language::Tsx, Backend::Text(Box::new(text::Check))),
    ];
    RuleDef { meta: META, backends }
}
