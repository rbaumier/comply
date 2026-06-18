//! playwright-no-useless-not — disallow `not` when a direct matcher exists.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-useless-not",
    description: "Using `.not.toBeVisible()` when `.toBeHidden()` exists is needlessly indirect.",
    remediation: "Use the direct matcher instead of negating: \
                  `toBeHidden` instead of `not.toBeVisible`, \
                  `toBeDisabled` instead of `not.toBeEnabled`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-useless-not.md",
    ),
    categories: &["testing"],

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
