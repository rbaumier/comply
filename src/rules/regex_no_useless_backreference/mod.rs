//! regex-no-useless-backreference

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-backreference",
    description: "Backreference is always replaced by the empty string because it references itself or a group that has not yet been matched.",
    remediation: "Remove the useless backreference or restructure the regex so the referenced group is matched before the backreference.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-backreference.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
