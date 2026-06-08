//! eslint-comments-no-duplicate-disable — same rule disabled twice.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "eslint-comments-no-duplicate-disable",
    description: "Disabling the same rule twice (or more) in one comment is a typo, not extra suppression.",
    remediation: "Remove the duplicate rule id. If you really want to express \"this is here on purpose\", add a justification comment after `--`.",
    severity: Severity::Warning,
    doc_url: Some("https://eslint-community.github.io/eslint-plugin-eslint-comments/rules/no-duplicate-disable.html"),
    categories: &["lint-comments"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
