//! ts-no-unused-generic-parameter — generic parameters must appear in
//! parameters or return type.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unused-generic-parameter",
    description: "Generic type parameter is not referenced anywhere in the function signature.",
    remediation: "Remove the unused type parameter, or reference it from a parameter/return type so it actually contributes to inference.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
