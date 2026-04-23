//! no-valueof-field

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-valueof-field",
    description: "Do not define `valueOf` as a method or property on a class, interface, or object literal.",
    remediation: "Avoid overriding valueOf, use explicit conversion methods",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
