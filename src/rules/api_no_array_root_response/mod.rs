//! api-no-array-root-response — JSON responses must not be a bare array.
//!
//! Returning a root-level array (`Response.json([...])`) locks the
//! response shape to the array form. Wrapping in an object
//! (`{ data: [...], total: n }`) keeps the contract extensible: paging
//! metadata, links, and flags can be added later without breaking
//! clients.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-array-root-response",
    description: "API endpoints must not return a root-level JSON array — wrap in an object for extensibility.",
    remediation: "Return `{ data: [...], total: n }` instead of a bare array.",
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
