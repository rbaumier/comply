//! prefer-code-point

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-code-point",
    description: "Prefer `String#codePointAt()` over `String#charCodeAt()` and `String.fromCodePoint()` over `String.fromCharCode()`.",
    remediation: "Use `codePointAt()` instead of `charCodeAt()` and `String.fromCodePoint()` instead of `String.fromCharCode()`. The code-point variants handle full Unicode (including astral symbols) correctly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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
