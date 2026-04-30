//! rust-no-panic-macros — no `panic!`/`todo!`/`unimplemented!`/`unreachable!`.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-panic-macros",
    description: "No `panic!` / `todo!` / `unimplemented!` / `unreachable!` in production.",
    remediation: "Replace the macro with a typed Result error. `todo!()` and \
                  `unimplemented!()` mark placeholders that must not ship. \
                  `unreachable!()` should only mark compiler-proven impossible \
                  states with a `// Impossible: ...` comment. Tests are \
                  exempted — panicking in a `#[test]` is a clean failure.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
