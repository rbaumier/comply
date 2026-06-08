//! regex-no-invisible-character

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-invisible-character",
    description: "Invisible Unicode characters in regex (zero-width joiners, soft hyphens, etc.) are hard to spot and usually unintended.",
    remediation: "Use explicit Unicode escapes (`\\u{200D}`) instead of embedding invisible characters directly.",
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
