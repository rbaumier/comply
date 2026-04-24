//! ts-no-as-narrowing — forbid `as` used to narrow types.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-as-narrowing",
    description: "`as` should not be used to narrow types; use type predicates or `in` checks.",
    remediation: "Replace `x as NarrowType` with a user-defined type guard (`x is NarrowType`) or an `in`/`typeof`/`instanceof` check.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
