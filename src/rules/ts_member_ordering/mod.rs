//! ts-member-ordering — require a consistent order for class/interface members.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-member-ordering",
    description: "Class and interface members should follow a consistent order: signatures, fields, constructors, methods.",
    remediation: "Re-order members: put signatures first, then fields, then constructors, then methods.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/member-ordering"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
