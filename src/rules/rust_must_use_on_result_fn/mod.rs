//! rust-must-use-on-result-fn — `#[must_use]` on pub Result-returning fns.
//!
//! `Result` itself is `#[must_use]`, but that only fires when a
//! caller writes `some_fn(); // dropped Result`. The lint does not
//! propagate through `?`, early returns, or `let _ = ...`, and does
//! not flag anyone who silently discards the return value of a
//! `fn foo() -> Result<T, E>` through a method chain. Marking the
//! function itself `#[must_use]` closes that gap.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-must-use-on-result-fn",
    description: "Public functions returning `Result` should be `#[must_use]` so callers can't silently discard errors.",
    remediation: "Add `#[must_use]` above the function definition.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
