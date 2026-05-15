//! sonarjs-no-gratuitous-expressions — always-true / always-false conditions.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sonarjs-no-gratuitous-expressions",
    description: "Boolean expression that always evaluates to the same value — dead branch or buggy condition.",
    remediation: "Remove the constant branch, or fix the condition to actually depend on a runtime value.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/SonarSource/eslint-plugin-sonarjs/blob/master/docs/rules/no-gratuitous-expressions.md"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
