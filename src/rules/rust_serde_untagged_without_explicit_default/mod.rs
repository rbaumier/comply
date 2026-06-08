//! rust-serde-untagged-without-explicit-default — `#[serde(untagged)]` enums
//! whose variants contain `Option<T>` fields silently match an empty input.
//! `#[serde(default)]` makes that intent explicit and protects the variant
//! ordering from unintended fall-through.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-serde-untagged-without-explicit-default",
    description: "`#[serde(untagged)]` variant with `Option<T>` field needs `#[serde(default)]`.",
    remediation: "Add `#[serde(default)]` to the `Option<T>` field. With \
                  `#[serde(untagged)]`, serde tries each variant in source \
                  order and picks the first that deserializes — an \
                  `Option<T>` field can match an empty input by accident, \
                  which silently shadows later variants. The explicit \
                  default makes the matching deterministic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust", "serde"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
