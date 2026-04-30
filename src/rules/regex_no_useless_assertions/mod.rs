//! regex-no-useless-assertions

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-assertions",
    description: "Regex contains an assertion that is always true or always false, making it useless.",
    remediation: "Remove the useless assertion or restructure the pattern so the assertion is meaningful.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-assertions.html",
    ),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
