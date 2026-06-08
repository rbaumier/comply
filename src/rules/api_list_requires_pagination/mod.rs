//! api-list-requires-pagination — GET list handlers must expose a
//! pagination parameter.
//!
//! Unbounded list endpoints are a latent DoS: a single call can fetch
//! the full table and pin memory / DB resources. Requiring a pagination
//! primitive (`limit`, `cursor`, `page`, `pageSize`, `offset`,
//! `per_page`) forces the author to think about result-set size at the
//! API boundary.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-list-requires-pagination",
    description: "List endpoints must support pagination to prevent unbounded result sets.",
    remediation: "Add `limit`/`cursor` or `page`/`pageSize` parameters to the handler.",
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
        ],
    }
}
