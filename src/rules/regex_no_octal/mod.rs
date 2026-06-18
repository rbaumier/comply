//! regex-no-octal

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-octal",
    description: "Octal escapes in regex (`\\1`, `\\12`) are ambiguous — they could be backreferences or octal character codes.",
    remediation: "Use named backreferences (`\\k<name>`) or explicit Unicode escapes (`\\u{...}`) instead of bare octal sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],

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
