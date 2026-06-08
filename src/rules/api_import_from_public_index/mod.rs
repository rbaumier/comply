//! api-import-from-public-index — cross-feature imports must go
//! through the feature's public index.
//!
//! Reaching into a sibling feature's internals
//! (`../../users/db/queries`) couples consumers to implementation
//! details that the owning feature is free to rearrange. Importing
//! from the feature root (`../../users`) routes through the curated
//! public surface and keeps module boundaries honest.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-import-from-public-index",
    description: "Cross-feature imports must go through the public index, not internal files.",
    remediation: "Import from `../users` (index) instead of `../users/db/queries`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api", "architecture"],

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
