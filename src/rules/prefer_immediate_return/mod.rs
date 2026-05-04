//! prefer-immediate-return — `let X = expr; return X;` → `return expr;`.
//!
//! Detection is AST-based: each backend walks `block` (Rust) /
//! `statement_block` (TS) nodes and looks at consecutive *named
//! children*, never at consecutive source lines. See the backend
//! docblocks for the full matching logic and rationale.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-immediate-return",
    description: "Variable is assigned and immediately returned.",
    remediation: "Return the expression directly: `return computeValue()` instead of `const result = computeValue(); return result;`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
