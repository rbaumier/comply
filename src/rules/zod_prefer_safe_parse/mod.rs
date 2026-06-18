//! zod-prefer-safe-parse — route handlers should not let `ZodError` escape.

mod oxc_typescript;


use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-safe-parse",
    description: "`.parse()` in a route handler throws `ZodError` unhandled — use `.safeParse()` instead.",
    remediation: "Use `.safeParse()` and handle `!result.success` to return a structured 400 response.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod", "api"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
