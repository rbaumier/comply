//! rust-panic-in-drop — `panic!` / `unwrap` / `expect` inside `impl Drop`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-panic-in-drop",
    description: "`Drop::drop` body contains `panic!` / `unwrap` / `expect` / `assert!`.",
    remediation: "Panicking inside `Drop` during another panic aborts the \
                  process. `Drop` runs on every error path — it must never \
                  fail. Log the error, swallow it, or use a fallible \
                  alternative invoked before the value is dropped.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
