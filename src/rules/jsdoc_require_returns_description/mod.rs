//! jsdoc/require-returns-description — imported from eslint-plugin-jsdoc.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-returns-description",
    description: "`@returns` tag must have a description.",
    remediation: "Describe what the function returns after the optional `{type}`: `@returns {User} the updated user, or null if not found`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-returns-description.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
