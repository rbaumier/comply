//! regex-no-useless-dollar-replacements

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-dollar-replacements",
    description: "Replacement string references a capturing group that does not exist in the regex.",
    remediation: "Fix the replacement reference to match an existing capturing group, or use `$$` to insert a literal dollar sign.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-dollar-replacements.html",
    ),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
