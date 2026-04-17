//! regex-no-optional-assertion

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-optional-assertion",
    description: "Assertion inside an optional group is effectively ignored and does not change the pattern.",
    remediation: "Remove the assertion or change the parent quantifier so the assertion is always evaluated.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-optional-assertion.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
