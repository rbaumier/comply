//! no-catch-log-rethrow — flag `catch { log(e); throw e; }` patterns.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-catch-log-rethrow",
    description: "Catch that only logs and rethrows — the log duplicates the uncaught handler.",
    remediation: "Remove the catch block entirely. The error will propagate to the \
                  top-level handler which already logs it, so the local log just \
                  produces duplicate stack traces. Catch only when you add value: \
                  wrap with context, recover, translate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["error-handling"],

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
