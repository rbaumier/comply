//! rust-prefer-arc-clone — prefer `Arc::clone(&x)` over `x.clone()` for Arc values.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-arc-clone",
    description: "`.clone()` on an `Arc` is visually identical to a deep clone — use `Arc::clone(&x)` to signal the cheap reference-count bump.",
    remediation: "Replace `x.clone()` with `Arc::clone(&x)` to make the intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
