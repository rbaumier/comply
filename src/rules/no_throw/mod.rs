//! no-throw — flag every `throw` statement.
//!
//! Applies to TS, TSX, and JS — all three use the same tree-sitter backend
//! (`typescript.rs`) since JS is a strict subset of the TS grammar and TSX
//! exposes the same `throw_statement` node kind.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-throw",
    description: "Never throw — Result<T, E> surfaces errors as values.",
    remediation: "Use Result<T, E> instead of throw — surface errors as values, \
                  not exceptions. Callers can't see thrown errors in the type signature.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};
pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "clippy::panic")
}
