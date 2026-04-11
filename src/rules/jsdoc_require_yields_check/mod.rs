//! jsdoc-require-yields-check

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-yields-check",
    description: "A `@yields` tag must be present when a generator function has a `yield` statement.",
    remediation: "Add a `@yields` tag since this generator contains `yield` expressions.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-yields-check.md"),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    let backends: Vec<_> = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
    ]
    .into_iter()
    .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
    .collect();
    RuleDef {
        meta: META,
        backends,
    }
}
