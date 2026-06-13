//! tanstack-query-no-async-query-fn-without-await — flag
//! `queryFn: async () => fetch(...)` patterns that return the fetch
//! promise but never await it. The query resolves with the unconsumed
//! Response, so error handling and JSON parsing run outside the query
//! lifecycle.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-async-query-fn-without-await",
    description: "`queryFn: async () => fetch(...)` returns an unconsumed Response.",
    remediation: "Await the fetch and parse the body inside the query function: \
                  `queryFn: async () => { const r = await fetch(url); return r.json(); }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],

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
