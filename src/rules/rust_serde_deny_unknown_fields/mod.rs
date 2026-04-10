//! rust-serde-deny-unknown-fields — opt in to typo-catching deserialization.
//!
//! By default, serde silently ignores any field present in the input
//! but missing from the struct. So a typo in a config file or API
//! payload (`raate` instead of `rate`) deserializes successfully and
//! you find out hours later when the rate field is the type's default.
//!
//! `#[serde(deny_unknown_fields)]` flips this behavior: unknown fields
//! become parse errors. Always opt in on types deserialized from
//! external input.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-serde-deny-unknown-fields",
    description: "Deserialize-derive structs need `#[serde(deny_unknown_fields)]`.",
    remediation: "Add `#[serde(deny_unknown_fields)]` above the struct \
                  definition. Without it, typos in input files or API \
                  payloads deserialize silently — fields the type doesn't \
                  know about are dropped, and the user finds out later.",
    severity: Severity::Warning,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
