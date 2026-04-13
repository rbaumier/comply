//! prefer-immediate-return — `let X = expr; return X;` → `return expr;`.
//!
//! Detection is AST-based: each backend walks `block` (Rust) /
//! `statement_block` (TS) nodes and looks at consecutive *named
//! children*, never at consecutive source lines. See the backend
//! docblocks for the full matching logic and rationale.

mod rust;
mod typescript;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-immediate-return",
    description: "Variable is assigned and immediately returned.",
    remediation: "Return the expression directly: `return computeValue()` instead of `const result = computeValue(); return result;`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
