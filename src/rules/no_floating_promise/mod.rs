//! no-floating-promise — flag promise-returning calls whose result is
//! discarded at statement level.

mod oxc_typescript;
mod shared;
#[cfg(test)]
mod typescript;

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
    severity: Severity::Warning,
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
