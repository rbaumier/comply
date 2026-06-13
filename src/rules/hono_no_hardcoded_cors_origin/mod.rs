//! hono-no-hardcoded-cors-origin — CORS origin should not be hardcoded.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-no-hardcoded-cors-origin",
    description: "CORS origin is a hardcoded string literal — environments share the same allowed origin.",
    remediation: "Read the origin from an environment variable or per-environment config (e.g. `cors({ origin: env.CORS_ORIGIN })`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["hono", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
