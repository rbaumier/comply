//! no-throw — flag every `throw` statement.
//!
//! Applies to TS, TSX, and JS — all three use the same tree-sitter backend
//! (`typescript.rs`) since JS is a strict subset of the TS grammar and TSX
//! exposes the same `throw_statement` node kind.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-throw",
    description: "Never throw — Result<T, E> surfaces errors as values.",
    remediation: "Use Result<T, E> instead of throw — surface errors as values, \
                  not exceptions. Callers can't see thrown errors in the type signature.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};
pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::Clippy { lint: "clippy::panic" }),
        ],
    }
}
