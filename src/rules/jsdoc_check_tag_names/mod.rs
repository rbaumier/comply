//! jsdoc/check-tag-names — imported from eslint-plugin-jsdoc.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/check-tag-names",
    description: "JSDoc tag names must be known (e.g. `@param`, `@returns`, …).",
    remediation: "Replace the unknown tag with a canonical JSDoc tag, or drop it. Common typos: `@arg` → `@param`, `@return` → `@returns`, `@desc` → `@description`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-tag-names.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
