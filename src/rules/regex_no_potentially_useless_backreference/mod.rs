//! regex-no-potentially-useless-backreference

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-potentially-useless-backreference",
    description: "Backreference may be useless because some paths to it do not go through the referenced group.",
    remediation: "Restructure the regex so all paths to the backreference pass through the referenced capturing group.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-potentially-useless-backreference.html",
    ),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
