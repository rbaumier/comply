//! max-function-lines — cap every function at 30 lines.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "max-function-lines",
    description: "Functions longer than 30 lines mix abstraction levels.",
    remediation: "Function exceeds 30 lines. Extract a named helper for the \
                  tail of the body — one level of abstraction per function.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            // Rust: delegated to clippy — see rust.rs for setup.
            (Language::Rust, Backend::Clippy { lint: "clippy::too_many_lines" }),
        ],
    }
}
