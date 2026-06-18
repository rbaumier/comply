//! elysia-deploy-no-graceful-shutdown

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deploy-no-graceful-shutdown",
    description: "Elysia server `.listen()` without graceful shutdown — in-flight requests are dropped on SIGTERM/SIGINT.",
    remediation: "Register a `process.on('SIGTERM', ...)` (and SIGINT) handler that calls `app.stop()` to drain connections before exit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["deployment", "elysia"],

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
