//! jsdoc-require-param-name

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-param-name",
    description: "Every `@param` tag must include a parameter name.",
    remediation: "Add the parameter name after the type annotation — `@param {string} name` instead of `@param {string}`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-param-name.md"),
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
