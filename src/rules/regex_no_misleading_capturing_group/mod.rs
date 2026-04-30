//! regex-no-misleading-capturing-group

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-misleading-capturing-group",
    description: "Capturing group matches different things at the start and end, which is misleading.",
    remediation: "Restructure the regex so the capturing group has a clear, unambiguous match.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-misleading-capturing-group.html",
    ),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
