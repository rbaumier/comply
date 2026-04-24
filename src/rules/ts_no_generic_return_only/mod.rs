//! ts-no-generic-return-only — forbid generics used only in return position.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-generic-return-only",
    description: "Generic type parameter appears only in the return type; it has no inference site.",
    remediation: "Remove the generic and return a concrete type, or add a parameter that references the generic so callers can drive inference.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
