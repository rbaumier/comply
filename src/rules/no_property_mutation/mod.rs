//! Forbids property mutation (`obj.prop = value`).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-property-mutation",
    description: "Forbids mutation of object properties.",
    remediation: "Use spread syntax `{ ...obj, prop: value }` or immutable update patterns.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional", "immutability"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
