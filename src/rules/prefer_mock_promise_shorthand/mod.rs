//! prefer-mock-promise-shorthand — flag `.mockImplementation(() => Promise.resolve/reject(x))`
//! and suggest the shorthand `.mockResolvedValue(x)` / `.mockRejectedValue(x)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-mock-promise-shorthand",
    description: "Prefer `.mockResolvedValue(x)` / `.mockRejectedValue(x)` over `.mockImplementation(() => Promise.resolve/reject(x))`.",
    remediation: "Use mockResolvedValue/mockRejectedValue instead",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/sindresorhus/eslint-plugin-unicorn/blob/main/docs/rules/prefer-mock-promise-shorthand.md",
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
