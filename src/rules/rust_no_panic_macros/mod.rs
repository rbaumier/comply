//! rust-no-panic-macros — no `panic!`/`todo!`/`unimplemented!`/`unreachable!`.

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
