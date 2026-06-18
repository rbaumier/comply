//! prefer-early-return

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-early-return",
    description: "Function body is wrapped in a single `if` — invert it as a guard clause.",
    remediation: "Invert the condition and return early: `if (!cond) return; ...` — reduces nesting and clarifies the happy path.",
    severity: Severity::Warning,
    doc_url: Some("https://eslint.org/docs/latest/rules/no-else-return"),
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
