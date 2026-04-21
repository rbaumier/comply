//! zod-prefer-safe-parse — route handlers should not let `ZodError` escape.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-safe-parse",
    description: "`.parse()` in a route handler throws `ZodError` unhandled — use `.safeParse()` instead.",
    remediation: "Use `.safeParse()` and handle `!result.success` to return a structured 400 response.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod", "api"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
