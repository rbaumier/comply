//! regex-no-trivially-nested-quantifier

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-trivially-nested-quantifier",
    description: "Two quantifiers are trivially nested and can be replaced with a single quantifier.",
    remediation: "Merge the nested quantifiers into a single equivalent quantifier.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-trivially-nested-quantifier.html",
    ),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
