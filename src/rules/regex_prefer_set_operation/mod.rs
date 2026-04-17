//! regex-prefer-set-operation

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-set-operation",
    description: "Lookaround combined with a character can be expressed more clearly using a set operation.",
    remediation: "Replace the lookaround pattern with a `v`-flag character class set operation.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/prefer-set-operation.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
