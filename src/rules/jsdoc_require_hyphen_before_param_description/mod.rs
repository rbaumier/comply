//! jsdoc/require-hyphen-before-param-description — imported from eslint-plugin-jsdoc.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-hyphen-before-param-description",
    description: "Separate the `@param` name from its description with a hyphen.",
    remediation: "Insert ` - ` between the param name and its description: `@param {string} id - the user id`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-hyphen-before-param-description.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
