//! promise-no-multiple-resolved — `new Promise()` executor calls resolve/reject more than once.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "promise-no-multiple-resolved",
    description: "A `new Promise()` executor that calls `resolve` or `reject` more than \
                  once silently discards every call after the first.",
    remediation: "Settle the promise exactly once. Use early `return` after `resolve()` / \
                  `reject()`, or restructure the executor to a single settlement point.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-promise/blob/main/docs/rules/no-multiple-resolved.md"),
    categories: &["promise"],
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
