//! regex-no-super-linear-move

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-super-linear-move",
    description: "Regex quantifier can cause quadratic runtime on certain inputs.",
    remediation: "Refactor the quantifier to avoid super-linear backtracking. Use atomic groups or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-super-linear-move.html"),
    categories: &["security", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
