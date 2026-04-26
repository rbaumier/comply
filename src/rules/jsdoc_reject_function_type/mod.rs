//! jsdoc-reject-function-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-reject-function-type",
    description: "JSDoc uses bare `Function` or `function` type instead of a specific function signature.",
    remediation: "Replace the bare `Function` type with a specific signature like `{(param: type) => returnType}`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/no-undefined-types.md"),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
