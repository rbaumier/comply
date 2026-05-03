//! elysia-test-listen-not-handle

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-test-listen-not-handle",
    description: "Elysia test boots a real server with `.listen()` and uses `fetch()` instead of `app.handle(new Request(...))`.",
    remediation: "Drive the app in tests with `app.handle(new Request(...))` — no port binding, faster, deterministic.",
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
