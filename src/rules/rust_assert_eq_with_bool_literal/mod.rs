//! rust-assert-eq-with-bool-literal — `assert_eq!(x, true)` should be `assert!(x)`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-assert-eq-with-bool-literal",
    description: "`assert_eq!` / `assert_ne!` compared against `true` / `false`.",
    remediation: "Use `assert!(x)` for `assert_eq!(x, true)` and `assert!(!x)` \
                  for `assert_eq!(x, false)`. The eq-form is noisier and \
                  produces a worse failure message (it shows `false != true` \
                  instead of just the failed condition).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
