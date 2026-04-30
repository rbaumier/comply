//! node-no-unhandled-rejection — rejection handlers must exit the process.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-unhandled-rejection",
    description: "`process.on('unhandledRejection', ...)` handlers should exit the process.",
    remediation: "Call `process.exit(1)` (or `process.exitCode = 1` + `throw err`) inside the \
                  handler. Continuing execution after an unhandled rejection leaves the process \
                  in an unknown state.",
    severity: Severity::Error,
    doc_url: Some("https://nodejs.org/api/process.html#event-unhandledrejection"),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
