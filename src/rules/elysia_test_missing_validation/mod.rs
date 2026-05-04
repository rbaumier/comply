//! elysia-test-missing-validation

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-test-missing-validation",
    description: "Test file declares a body schema but never asserts a 400/422 validation error.",
    remediation: "Add a test case that sends an invalid payload and asserts the route returns 400 (or 422 in `aot:false` mode).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing", "elysia"],
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
