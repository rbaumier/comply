//! ts-no-mixed-types — flag interfaces / object type aliases that mix
//! property signatures with method signatures.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-mixed-types",
    description: "Interfaces and type aliases should not mix property signatures with method signatures.",
    remediation: "Use consistent signatures: either all properties or all methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
