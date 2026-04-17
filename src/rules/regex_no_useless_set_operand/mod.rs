//! regex-no-useless-set-operand

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-set-operand",
    description: "Character class set operation has a useless operand that does not affect the result.",
    remediation: "Remove the useless operand from the set operation.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-set-operand.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
