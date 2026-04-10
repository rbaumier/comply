//! rust-string-as-error — `Result<T, String>` is stringly-typed.
//!
//! `String` as an error type collapses every failure mode into a
//! single opaque blob. The caller can't pattern-match on variants,
//! can't programmatically distinguish "not found" from "permission
//! denied", and can't add structured context. Define a real error
//! enum (with `thiserror::Error` if you want the boilerplate gone)
//! so the type system carries the failure shape.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-string-as-error",
    description: "`Result<T, String>` is stringly-typed and unmatchable.",
    remediation: "Define a proper error enum (use `thiserror::Error` for \
                  the boilerplate) and use it as the `E` parameter. \
                  String errors prevent callers from pattern-matching \
                  failure modes and lose all structured context.",
    severity: Severity::Warning,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
