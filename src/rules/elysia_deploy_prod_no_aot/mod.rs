//! elysia-deploy-prod-no-aot

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deploy-prod-no-aot",
    description: "`new Elysia({ ... })` configured without `aot: true` — production builds lose ahead-of-time compilation.",
    remediation: "Pass `aot: true` (or omit the flag if you intentionally want JIT) when constructing the Elysia instance used in production deployments.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "elysia"],

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
