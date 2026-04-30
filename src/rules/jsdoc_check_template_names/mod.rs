//! jsdoc/check-template-names — imported from eslint-plugin-jsdoc.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/check-template-names",
    description: "`@template` names must be referenced somewhere in the block.",
    remediation: "Use the declared type parameter inside a `@param` / `@returns` / `@type` tag, or remove the `@template` declaration.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-template-names.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
