//! elysia-group-deep-paths

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-group-deep-paths",
    description: "Deep route paths repeated across handlers should be grouped via `.group()` or a `prefix`.",
    remediation: "Wrap deep paths under `.group('/v1/users', g => g.get('/profile', ...))` or pass `prefix` to a sub-app.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],

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
