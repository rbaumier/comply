//! timeout-on-io — every I/O call needs a timeout.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "timeout-on-io",
    description: "I/O calls without a timeout can hang forever.",
    remediation: "Wrap the I/O call with `withTimeout(call, 5_000)` or pass \
                  `{ signal: AbortSignal.timeout(5_000) }`. Default \
                  timeouts: 5s for DB, 10s for external APIs, 30s for file ops.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],

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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
