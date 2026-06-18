//! elysia-numeric-body-no-coerce

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-numeric-body-no-coerce",
    description: "`t.Number()` inside a `body:` schema rejects numeric strings — use `t.Numeric()` for form-encoded payloads.",
    remediation: "Replace `t.Number()` with `t.Numeric()` in `body:` schemas so multipart/urlencoded numeric fields coerce.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],

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
