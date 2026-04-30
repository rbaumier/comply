//! jsdoc-reject-any-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-reject-any-type",
    description: "JSDoc uses `*` or `any` as a type, which defeats the purpose of type documentation.",
    remediation: "Replace the `*`/`any` type annotation with a specific type.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/no-undefined-types.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
