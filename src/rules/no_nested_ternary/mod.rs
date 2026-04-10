//! no-nested-ternary — flag ternaries whose parent is also a ternary.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-nested-ternary",
    description: "Nested ternaries are hard to read and easy to misparse.",
    remediation: "Nested ternary — extract to if/else or a named variable for each branch.",
    severity: Severity::Error,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
