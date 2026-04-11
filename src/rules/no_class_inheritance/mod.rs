//! no-class-inheritance

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-class-inheritance",
    description: "Class inheritance (`extends`) creates tight coupling — prefer composition over inheritance.",
    remediation: "Use composition, mixins, or dependency injection instead of class inheritance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
