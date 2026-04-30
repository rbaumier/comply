//! jsdoc/require-tags

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-tags",
    description: "Exported function JSDoc must document parameters and return when relevant.",
    remediation: "Add `@param` tags for each parameter and `@returns` for non-void return values.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-tags.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
