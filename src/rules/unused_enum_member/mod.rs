//! unused-enum-member

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "unused-enum-member",
    description: "Enum member is declared but never referenced in this file.",
    remediation: "Remove the unused member, or reference it where the enum is consumed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["clean-code"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
