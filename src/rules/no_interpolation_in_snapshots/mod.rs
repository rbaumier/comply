//! no-interpolation-in-snapshots

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-interpolation-in-snapshots",
    description: "Template literals passed to snapshot matchers should not contain interpolation.",
    remediation: "Don't use interpolation in snapshot matchers",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jest-community/eslint-plugin-jest/blob/main/docs/rules/no-interpolation-in-snapshots.md",
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
