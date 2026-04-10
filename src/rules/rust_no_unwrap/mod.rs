//! rust-no-unwrap — no `.unwrap()` or `.expect()` in production code.
//!
//! Tree-sitter-based backend detects `.unwrap()` / `.expect()` calls
//! outside of test contexts. Tests are exempted (panicking inside a
//! `#[test]` is a clean failure mode).

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-unwrap",
    description: "No `.unwrap()` / `.expect()` in production code.",
    remediation: "Handle the None / Err case explicitly. Use `?` with \
                  proper error propagation, or `unwrap_or_else` with a \
                  meaningful fallback. `unwrap()` turns runtime conditions \
                  into crashes. Tests are exempted — panicking inside a \
                  `#[test]` is a clean failure.",
    severity: Severity::Error,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
