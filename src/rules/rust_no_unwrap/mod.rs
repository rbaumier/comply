//! rust-no-unwrap — no `.unwrap()` or `.expect()` in production code.
//!
//! Tree-sitter-based backend detects `.unwrap()` / `.expect()` calls
//! outside of test contexts. Tests are exempted (panicking inside a
//! `#[test]` is a clean failure mode).

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
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
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
