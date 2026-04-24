//! react-require-versioned-storage-key — `localStorage.setItem("k", ...)` without `:vN` suffix.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-require-versioned-storage-key",
    description: "`localStorage.setItem` uses a literal key without a `:vN` version suffix, \
                  so a shape change to the stored value cannot be rolled forward.",
    remediation: "Add a version suffix (e.g. `\"settings:v1\"`) and bump it when the \
                  serialized shape changes so old entries can be migrated or dropped.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
