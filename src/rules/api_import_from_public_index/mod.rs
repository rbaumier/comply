//! api-import-from-public-index — cross-feature imports must go
//! through the feature's public index.
//!
//! Reaching into a sibling feature's internals
//! (`../../users/db/queries`) couples consumers to implementation
//! details that the owning feature is free to rearrange. Importing
//! from the feature root (`../../users`) routes through the curated
//! public surface and keeps module boundaries honest.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-import-from-public-index",
    description: "Cross-feature imports must go through the public index, not internal files.",
    remediation: "Import from `../users` (index) instead of `../users/db/queries`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api", "architecture"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
