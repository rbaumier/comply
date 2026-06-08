//! regex-no-empty-string-match

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-string-match",
    description: "Regex that matches the empty string used in `.split()` or `.replace()`.",
    remediation: "A pattern like `*`, `?`, or `{0,}` can match zero characters, causing unexpected splits or replacements. Use `+` or `{1,}` instead, or add anchors.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],

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
