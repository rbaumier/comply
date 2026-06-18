//! ts-no-dynamic-delete — disallow `delete` on computed key expressions.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-dynamic-delete",
    description: "Using `delete` on a computed key is error-prone — use `Map` or `Set` instead.",
    remediation: "Remove the dynamic `delete` and use a `Map`/`Set`, or delete a static key.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-dynamic-delete/"),
    categories: &["typescript"],

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
