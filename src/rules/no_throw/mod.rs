//! no-throw — flag every `throw` statement.
//!
//! Applies to TS, TSX, and JS — all three use the same tree-sitter backend
//! (`typescript.rs`) since JS is a strict subset of the TS grammar and TSX
//! exposes the same `throw_statement` node kind.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-throw",
    description: "Never throw — Result<T, E> surfaces errors as values.",
    remediation: "Use Result<T, E> instead of throw — surface errors as values, \
                  not exceptions. Callers can't see thrown errors in the type signature.",
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
        ],
    }
}
