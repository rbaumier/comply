//! promise-catch-or-return — top-level Promise must `.catch()` or be returned.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "promise-catch-or-return",
    description: "A floating Promise chain without `.catch()` or `return` swallows rejection at the runtime's discretion.",
    remediation: "Either `.catch(handler)` the chain, `return` it from the function so the caller deals with rejection, or `void promise.catch(...)` if rejection is genuinely ignorable.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-promise/blob/main/docs/rules/catch-or-return.md"),
    categories: &["promise"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
