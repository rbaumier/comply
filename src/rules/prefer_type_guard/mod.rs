//! prefer-type-guard

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-guard",
    description: "Functions named `isX` returning `boolean` with `typeof`/`instanceof` should use type predicates.",
    remediation: "Change the return type from `: boolean` to `: x is Type` to enable type narrowing at call sites.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
