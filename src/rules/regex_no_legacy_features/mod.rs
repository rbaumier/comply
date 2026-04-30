//! regex-no-legacy-features

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-legacy-features",
    description: "Regex uses legacy RegExp static properties like `RegExp.$1` or `RegExp.lastMatch`.",
    remediation: "Avoid legacy RegExp static properties. Use capturing groups and match results instead.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-legacy-features.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
