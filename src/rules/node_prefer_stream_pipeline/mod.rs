//! node-prefer-stream-pipeline — prefer `pipeline()` over `.pipe()` chaining.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-prefer-stream-pipeline",
    description: "`stream.pipe()` chains leak resources on error — `pipeline()` cleans them up.",
    remediation: "Replace `a.pipe(b).pipe(c)` with `await pipeline(a, b, c)` from `node:stream/promises`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://nodejs.org/api/stream.html#streampipelinesource-transforms-destination-callback",
    ),
    categories: &["node"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
