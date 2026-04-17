//! regex-no-trivially-nested-assertion

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-trivially-nested-assertion",
    description: "Lookaround assertion is trivially nested inside another lookaround of the same kind.",
    remediation: "Merge the nested lookaround into its parent or simplify the structure.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-trivially-nested-assertion.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
