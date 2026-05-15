//! ts-no-misused-promises — async callback in a void-returning slot.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-misused-promises",
    description: "Async function passed to a callback slot that expects void — the returned Promise is dropped.",
    remediation: "Either drop the `async` and use `.then(...)` explicitly, or wrap the body in a `void (async () => { ... })()` IIFE so the unhandled Promise is intentional.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-misused-promises/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
