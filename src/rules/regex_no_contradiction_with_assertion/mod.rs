//! regex-no-contradiction-with-assertion

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-contradiction-with-assertion",
    description: "Regex contains an assertion that contradicts the pattern around it, making the branch unmatchable.",
    remediation: "Remove or fix the contradictory assertion so the branch can actually match.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-contradiction-with-assertion.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
