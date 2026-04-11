//! ts-no-wrapper-object-types — flag `String`, `Number`, `Boolean`, etc. in type positions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-wrapper-object-types",
    description: "Use lowercase primitives (`string`, `number`, `boolean`) instead of wrapper object types.",
    remediation: "Replace `String` with `string`, `Number` with `number`, `Boolean` with `boolean`, \
                  `Object` with `object`, `Symbol` with `symbol`, `BigInt` with `bigint`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
