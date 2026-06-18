//! try-catch-new-url — flag `new URL(...)` outside a try block.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "try-catch-new-url",
    description: "`new URL(...)` can throw — wrap it in try/catch or use `URL.canParse`.",
    remediation: "`new URL(invalid)` throws a TypeError. Either wrap in try/catch \
                  and handle the invalid-URL case, or gate with `URL.canParse(s)` \
                  before constructing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["error-handling"],

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
