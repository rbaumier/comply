//! Forbids mutation of `this` properties outside constructor.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-this-mutation",
    description: "Forbids mutation of `this` properties outside constructor.",
    remediation: "Initialize all properties in the constructor. Use immutable patterns or return new instances.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional", "immutability"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
