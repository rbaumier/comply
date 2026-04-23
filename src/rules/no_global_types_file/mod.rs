//! Forbids global `types.ts` files — colocate types with their usage.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-global-types-file",
    description: "Forbids global `types.ts` files at project root or shared locations.",
    remediation: "Colocate types with the code that uses them, or use domain-specific type files.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["architecture"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
