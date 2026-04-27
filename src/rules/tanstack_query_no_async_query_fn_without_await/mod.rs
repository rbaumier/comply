//! tanstack-query-no-async-query-fn-without-await — flag
//! `queryFn: async () => fetch(...)` patterns that return the fetch
//! promise but never await it. The query resolves with the unconsumed
//! Response, so error handling and JSON parsing run outside the query
//! lifecycle.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-async-query-fn-without-await",
    description: "`queryFn: async () => fetch(...)` returns an unconsumed Response.",
    remediation: "Await the fetch and parse the body inside the query function: \
                  `queryFn: async () => { const r = await fetch(url); return r.json(); }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
