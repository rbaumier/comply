//! Shared test fixtures for the in-process tree-sitter rules.
//!
//! Every rule's `typescript.rs` / `rust.rs` test module previously
//! contained the same ~10-line `fn run_on(source: &str) -> Vec<Diagnostic>`
//! boilerplate that creates a parser, sets the language, parses the
//! source, and dispatches to the rule's `Check`. Forty rules × ten
//! lines = 400 lines of pure copy-paste, all flagged by jscpd as a
//! Rule of Three (Rule of Forty, really).
//!
//! This module exposes:
//!
//! - `run_ts(source, check)` — TypeScript grammar (also covers `.ts` and
//!   plain JS, since the TypeScript grammar is a strict superset).
//! - `run_tsx(source, check)` — TSX/JSX grammar.
//! - `run_rust(source, check)` — Rust grammar.
//!
//! Each helper builds the right parser, runs `check.check(&ctx, &tree)`,
//! and returns the diagnostics. Test bodies become one line:
//!
//! ```ignore
//! let diags = test_helpers::run_ts("function f() { throw 1; }", &Check);
//! ```
//!
//! Compiled out of release builds via `#[cfg(test)]` at the registry
//! site (`rules/mod.rs`).

#![cfg(test)]

use std::path::Path;

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};

/// Run a tree-sitter `Check` against `source` parsed with the standard
/// TypeScript grammar (covers `.ts` and plain JS).
#[must_use]
pub fn run_ts(source: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "t.ts",
    )
}

/// Same as `run_ts` but with a custom fake filename. Use this when
/// the rule filters on the file path (e.g. Playwright `*.test.ts`).
#[must_use]
pub fn run_ts_with_path(source: &str, check: &dyn AstCheck, fake_path: &str) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        fake_path,
    )
}

/// Same as `run_ts` but with the TSX/JSX grammar variant. Use this when
/// the rule under test inspects JSX-specific node kinds.
#[must_use]
pub fn run_tsx(source: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        "t.tsx",
    )
}

/// Run a tree-sitter `Check` against `source` parsed with the Rust
/// grammar.
#[must_use]
pub fn run_rust(source: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_rust::LANGUAGE.into(),
        "t.rs",
    )
}

fn run_with_grammar(
    source: &str,
    check: &dyn AstCheck,
    grammar: tree_sitter::Language,
    fake_path: &str,
) -> Vec<Diagnostic> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar).expect("grammar should load");
    let tree = parser.parse(source, None).expect("parser should produce a tree");
    check.check(&CheckCtx::for_test(Path::new(fake_path), source), &tree)
}
