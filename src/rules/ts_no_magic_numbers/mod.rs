//! no-magic-numbers — disallow magic numbers in TS/JS.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-magic-numbers",
    description: "Magic numbers in TS/JS make code harder to understand — use named constants instead.",
    remediation: "Extract the number into a named `const`. TS enums, numeric literal types, `readonly` properties, and common values (0, 1, -1) are allowed.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-magic-numbers"),
    categories: &["code-quality"],

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
