//! regex-no-missing-g-flag

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-missing-g-flag",
    description: "Regex used with a method that expects the global flag but the g flag is missing.",
    remediation: "Add the `g` flag to the regex or use a method that does not require it.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-missing-g-flag.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
