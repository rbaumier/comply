//! rust-no-todo-macro — `todo!()` invocations in non-test code.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-todo-macro",
    description: "No `todo!()` macro invocations in production code.",
    remediation: "Replace `todo!()` with the actual implementation, or with \
                  a typed `Result` error if the path is intentionally \
                  unsupported. `todo!()` is a placeholder marker — when \
                  it ships it turns into a runtime panic. Tests are \
                  exempted (panicking inside a `#[test]` is a clean \
                  failure mode).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
