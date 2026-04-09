//! jsdoc-on-exported — every exported function needs a JSDoc block.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-on-exported",
    description: "Exported functions must document their public contract.",
    remediation: "Add a `/** ... */` JSDoc block above the export, \
                  describing what the function does, its parameters, and \
                  what it returns. Include an @example when the call site \
                  isn't obvious.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            // Rust: rustc's built-in `missing_docs` lint enforces doc
            // comments on every `pub` item. See rust.rs for setup.
            (Language::Rust, Backend::Clippy { lint: "missing_docs" }),
        ],
    }
}
