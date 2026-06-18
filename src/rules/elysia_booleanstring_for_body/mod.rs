//! elysia-booleanstring-for-body

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-booleanstring-for-body",
    description: "`t.Boolean()` inside a `body:` schema rejects `\"true\"` / `\"false\"` form fields.",
    remediation: "Use `t.BooleanString()` for form-encoded payloads where booleans arrive as strings.",
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
