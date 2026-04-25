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
use crate::project::ProjectCtx;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::file_ctx::FileCtx;

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

/// Same as `run_ts` but with a caller-supplied `ProjectCtx` + fake path. Use
/// when a cross-file rule needs a populated `ImportIndex` to query.
#[must_use]
pub fn run_ts_with_project_and_path(
    source: &str,
    check: &dyn AstCheck,
    project: &ProjectCtx,
    fake_path: &Path,
) -> Vec<Diagnostic> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("grammar should load");
    let tree = parser.parse(source, None).expect("parser should produce a tree");
    check.check(
        &CheckCtx::for_test_with_project(fake_path, source, project),
        &tree,
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

/// TSX variant that lets the test supply a pre-built `FileCtx`. Use when
/// the rule consumes `ctx.file.*` (RSC classification, directives,
/// path segments).
#[must_use]
pub fn run_tsx_with_file_ctx(
    source: &str,
    check: &dyn AstCheck,
    file: &FileCtx,
) -> Vec<Diagnostic> {
    run_with_grammar_and_file(
        source,
        check,
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        "t.tsx",
        file,
    )
}

/// TSX variant that lets the test supply both a `ProjectCtx` and a
/// `FileCtx`. Use when a rule is framework-scoped (Next.js, Nuxt, …) in
/// addition to consuming `ctx.file.*`.
#[must_use]
pub fn run_tsx_with_project_and_file(
    source: &str,
    check: &dyn AstCheck,
    project: &ProjectCtx,
    file: &FileCtx,
) -> Vec<Diagnostic> {
    run_tsx_with_project_file_and_path(source, check, project, file, "t.tsx")
}

/// Same as `run_tsx_with_project_and_file` but with a caller-supplied fake
/// path. Use when the rule also inspects `ctx.path` (filename patterns like
/// `layout.tsx`).
#[must_use]
pub fn run_tsx_with_project_file_and_path(
    source: &str,
    check: &dyn AstCheck,
    project: &ProjectCtx,
    file: &FileCtx,
    fake_path: &str,
) -> Vec<Diagnostic> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .expect("grammar should load");
    let tree = parser.parse(source, None).expect("parser should produce a tree");
    check.check(
        &CheckCtx::for_test_full(Path::new(fake_path), source, project, file),
        &tree,
    )
}

/// Run a tree-sitter `Check` against `source` parsed with the YAML
/// grammar. Used by Kubernetes manifest rules.
#[must_use]
pub fn run_yaml(source: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_yaml::LANGUAGE.into(),
        "manifest.yaml",
    )
}

/// Same as `run_yaml` but with a custom fake filename.
#[must_use]
pub fn run_yaml_with_path(source: &str, check: &dyn AstCheck, fake_path: &str) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_yaml::LANGUAGE.into(),
        fake_path,
    )
}

/// Run a tree-sitter `Check` against `source` parsed with the CSS
/// grammar. Use for rules that target `Language::Css`.
#[must_use]
pub fn run_css(source: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_css::LANGUAGE.into(),
        "t.css",
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

/// Same as `run_rust` but with a custom fake filename. Use this when
/// the rule filters on the file path (e.g. `src/main.rs` vs lib files).
#[must_use]
pub fn run_rust_with_path(source: &str, check: &dyn AstCheck, fake_path: &str) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_rust::LANGUAGE.into(),
        fake_path,
    )
}

/// Run a tree-sitter `Check` against `source` parsed with the Dockerfile
/// grammar.
#[must_use]
pub fn run_dockerfile(source: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    run_with_grammar(
        source,
        check,
        tree_sitter_dockerfile_updated::language(),
        "Dockerfile",
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


fn run_with_grammar_and_file(
    source: &str,
    check: &dyn AstCheck,
    grammar: tree_sitter::Language,
    fake_path: &str,
    file: &FileCtx,
) -> Vec<Diagnostic> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar).expect("grammar should load");
    let tree = parser.parse(source, None).expect("parser should produce a tree");
    check.check(
        &CheckCtx::for_test_with_file(Path::new(fake_path), source, file),
        &tree,
    )
}
