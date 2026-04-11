//! rust-unit-error-result — `Result<T, ()>` erases all error info.
//!
//! Returning `Result<T, ()>` tells the caller "something went wrong"
//! and nothing else — no kind, no message, no cause. The whole point
//! of `Result` is to carry the failure context across the call
//! boundary. If `()` is genuinely the right error type, use `Option`
//! instead — it expresses absence without pretending to be an error.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-unit-error-result",
    description: "`Result<T, ()>` discards every error detail.",
    remediation: "Define a real error type — even a tiny enum — and use \
                  it as the `E` parameter. If absence is the only failure \
                  mode, return `Option<T>` instead. `Result<T, ()>` is the \
                  worst of both worlds.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
