//! jsdoc/valid-types — imported from eslint-plugin-jsdoc.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/valid-types",
    description: "JSDoc `{...}` type expressions must be syntactically balanced and non-empty.",
    remediation: "Fix unbalanced braces/parens, remove empty `{}` types, and quote string-literal types that contain whitespace.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/valid-types.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
