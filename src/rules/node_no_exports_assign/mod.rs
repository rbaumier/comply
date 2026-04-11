//! node-no-exports-assign

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-exports-assign",
    description: "Direct assignment to `exports` variable is forbidden.",
    remediation: "Use `module.exports = ...` instead of `exports = ...`.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/no-exports-assign.md"),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
