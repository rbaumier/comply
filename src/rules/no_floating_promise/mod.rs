//! no-floating-promise — flag promise-returning calls whose result is
//! discarded at statement level.
//!
//! A call is treated as Promise-returning only on real, in-file evidence: a
//! `Promise.<combinator>(...)`, a bare call to a locally-declared `async`
//! function, or a `receiver.method(...)` whose same shape is `await`ed or
//! `.then`/`.catch`-chained elsewhere in the file. The method name alone is never
//! used, so a synchronous chainable method that shares an async-sounding name
//! (e.g. pdfkit's `doc.save()`) is not flagged.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-floating-promise",
    description: "Promise-returning call is used as a statement — rejection is ignored.",
    remediation: "`await` the promise, chain `.then/.catch`, pass it to \
                  `Promise.all`, or explicitly mark `void promise` if you \
                  intentionally ignore it. An unhandled rejection becomes an \
                  `UnhandledPromiseRejection` warning — and in Node 15+, crashes \
                  the process.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["async"],

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
