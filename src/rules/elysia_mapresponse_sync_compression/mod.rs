//! elysia-mapresponse-sync-compression

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-mapresponse-sync-compression",
    description: "`.mapResponse` handler runs synchronous compression that blocks the event loop.",
    remediation: "Use the async `gzip` / `deflate` from `zlib/promises` (or stream the response) instead of `gzipSync` / `deflateSync` inside `mapResponse`.",
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
