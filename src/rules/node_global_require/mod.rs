//! node-global-require

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "node-global-require",
    description: "`require()` calls should be at the top-level module scope.",
    remediation: "Move the `require()` call to the top level of the module.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/global-require.md"),
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
