//! boolean-naming — booleans must start with a predicate prefix.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "boolean-naming",
    description: "Boolean identifiers must start with is/has/should/can/will/did/was.",
    remediation: "Rename to convey the predicate: `ready` → `isReady` (TS) or \
                  `is_ready` (Rust). Use the positive form only — prefer \
                  `!isReady` over `isNotReady`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
