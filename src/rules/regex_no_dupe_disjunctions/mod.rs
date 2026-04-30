//! regex-no-dupe-disjunctions

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-dupe-disjunctions",
    description: "Regex contains duplicate alternatives that are redundant.",
    remediation: "Remove the duplicate alternative from the disjunction.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-dupe-disjunctions.html",
    ),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
