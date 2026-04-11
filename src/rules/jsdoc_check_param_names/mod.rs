//! jsdoc-check-param-names

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-check-param-names",
    description: "JSDoc `@param` names must match actual function parameters.",
    remediation: "Update the `@param` tag name to match the function signature. Stale or mismatched param docs mislead callers.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
