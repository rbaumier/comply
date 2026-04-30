//! jsdoc/check-types — imported from eslint-plugin-jsdoc.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc/check-types",
    description: "Prefer lowercase primitives in JSDoc types (e.g. `string` over `String`).",
    remediation: "Use the lowercase primitive: `String` → `string`, `Number` → `number`, `Boolean` → `boolean`, `Object` → `object`, `Symbol` → `symbol`, `Bigint` → `bigint`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/check-types.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
