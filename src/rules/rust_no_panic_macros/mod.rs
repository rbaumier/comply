//! rust-no-panic-macros — no `panic!`/`todo!`/`unimplemented!`/`unreachable!`.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-panic-macros",
    description: "No `panic!` / `todo!` / `unimplemented!` / `unreachable!` in production.",
    remediation: "Replace the macro with a typed Result error. `todo!()` and \
                  `unimplemented!()` mark placeholders that must not ship. \
                  `unreachable!()` should only mark compiler-proven impossible \
                  states with a `// Impossible: ...` comment. Enable \
                  `clippy::panic` + `clippy::todo` + `clippy::unimplemented` + \
                  `clippy::unreachable` in your crate root.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
