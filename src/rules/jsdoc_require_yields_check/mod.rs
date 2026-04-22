//! jsdoc/require-yields-check

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-yields-check",
    description: "`@yields` must match what the function actually yields.",
    remediation: "Either remove a `@yields` tag from a non-yielding function, or add a `yield` to a function documented with `@yields`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-yields-check.md",
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
