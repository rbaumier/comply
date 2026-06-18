//! prefer-string-trim-start-end

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-trim-start-end",
    description: "Prefer `String#trimStart()` / `String#trimEnd()` over the deprecated `trimLeft()` / `trimRight()`.",
    remediation: "Replace `.trimLeft()` with `.trimStart()` and `.trimRight()` with `.trimEnd()`. \
                  The `trimLeft`/`trimRight` aliases are deprecated in favor of the spec names.",
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
