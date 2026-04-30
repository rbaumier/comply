//! jsdoc/require-property — imported from eslint-plugin-jsdoc.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/require-property",
    description: "`@typedef` / `@interface` blocks for object types must declare at least one `@property`.",
    remediation: "Add `@property {Type} name - description` entries for each field of the typedef, or change the typedef's type to a non-object type.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/require-property.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
