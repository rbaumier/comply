//! next-no-unwrapped-cache

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-unwrapped-cache",
    description: "`unstable_cache` callbacks must handle errors — an unhandled throw poisons the cache.",
    remediation: "Wrap the inner work in try/catch and return a sentinel, or guard the call site with an error boundary.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/unstable_cache"),
    categories: &["nextjs", "reliability"],

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
