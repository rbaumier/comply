//! jsdoc/require-next-description

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-next-description",
    description: "Each @next tag must have a description.",
    remediation: "Add prose after the @next type (e.g. `@next {T} - what the next call receives`).",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-next-description.md",
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
