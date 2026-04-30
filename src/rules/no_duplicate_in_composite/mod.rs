//! no-duplicate-in-composite

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-in-composite",
    description: "Duplicate types in a union or intersection are redundant.",
    remediation: "Remove the duplicate type from the composite. `A | A` simplifies to `A`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
