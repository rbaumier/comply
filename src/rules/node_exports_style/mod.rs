//! node-exports-style

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "node-exports-style",
    description: "Enforce consistent `module.exports` vs `exports` usage.",
    remediation: "Use `module.exports` consistently. Do not use bare `exports` assignment.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/exports-style.md"),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
