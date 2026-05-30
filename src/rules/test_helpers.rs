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
use std::path::PathBuf;

use crate::diagnostic::Diagnostic;
use crate::diagnostic::Severity;
use crate::files::{Language, SourceFile};
use crate::project::ProjectCtx;
use crate::rules::RuleDef;
use crate::rules::backend::{AstCheck, Backend, CheckCtx};
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
    let tree = parser
        .parse(source, None)
        .expect("parser should produce a tree");
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

#[must_use]
pub fn run_ts_with_file_ctx(
    source: &str,
    check: &dyn AstCheck,
    file: &FileCtx,
) -> Vec<Diagnostic> {
    run_with_grammar_and_file(
        source,
        check,
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "t.ts",
        file,
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
    let tree = parser
        .parse(source, None)
        .expect("parser should produce a tree");
    check.check(
        &CheckCtx::for_test_full(Path::new(fake_path), source, project, file),
        &tree,
    )
}

/// TS variant with a caller-supplied framework name. Seeds a `ProjectCtx`
/// whose `has_framework(name)` returns true. Use for framework-scoped rules.
#[must_use]
pub fn run_ts_with_framework(
    source: &str,
    check: &dyn AstCheck,
    framework: &str,
) -> Vec<Diagnostic> {
    let project = ProjectCtx::for_test_with_framework(framework);
    run_ts_with_project_and_path(source, check, &project, Path::new("t.ts"))
}

/// TSX variant with a caller-supplied framework name.
#[must_use]
pub fn run_tsx_with_framework(
    source: &str,
    check: &dyn AstCheck,
    framework: &str,
) -> Vec<Diagnostic> {
    let project = ProjectCtx::for_test_with_framework(framework);
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .expect("grammar should load");
    let tree = parser
        .parse(source, None)
        .expect("parser should produce a tree");
    check.check(
        &CheckCtx::for_test_with_project(Path::new("t.tsx"), source, &project),
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
    run_with_grammar(source, check, tree_sitter_yaml::LANGUAGE.into(), fake_path)
}

/// YAML variant with a caller-supplied project context. Use for
/// Kubernetes rules that rely on the cross-file `K8sIndex`.
#[must_use]
pub fn run_yaml_with_project_and_path(
    source: &str,
    check: &dyn AstCheck,
    project: &ProjectCtx,
    fake_path: &Path,
) -> Vec<Diagnostic> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_yaml::LANGUAGE.into())
        .expect("grammar should load");
    let tree = parser
        .parse(source, None)
        .expect("parser should produce a tree");
    check.check(
        &CheckCtx::for_test_with_project(fake_path, source, project),
        &tree,
    )
}

/// Build a temporary YAML project for cross-manifest Kubernetes rule tests.
#[must_use]
pub fn k8s_project_from_sources(
    sources: &[(&str, &str)],
) -> (tempfile::TempDir, ProjectCtx, Vec<PathBuf>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut paths = Vec::new();
    let mut files = Vec::new();
    for (name, source) in sources {
        let path = dir.path().join(name);
        std::fs::write(&path, source).expect("write yaml fixture");
        paths.push(path.clone());
        files.push(SourceFile {
            path,
            language: Language::Yaml,
        });
    }
    let refs: Vec<&SourceFile> = files.iter().collect();
    let project = ProjectCtx::for_test_with_files(&refs);
    (dir, project, paths)
}

/// Assert a Rust-only rule delegates to exact Clippy lint set.
pub fn assert_clippy_rule(rule: RuleDef, id: &str, severity: Severity, expected_lints: &[&str]) {
    assert_eq!(rule.meta.id, id);
    assert_eq!(rule.meta.severity, severity);
    assert_eq!(rule.backends.len(), expected_lints.len());

    let actual_lints: Vec<&str> = rule
        .backends
        .iter()
        .map(|(language, backend)| {
            assert_eq!(*language, Language::Rust);
            match backend {
                Backend::Clippy { lint } => *lint,
                other => panic!("expected Clippy backend, got {other:?}"),
            }
        })
        .collect();

    assert_eq!(actual_lints, expected_lints);
}

/// Run a tree-sitter `Check` against `source` parsed with the CSS
/// grammar. Use for rules that target `Language::Css`.
#[must_use]
pub fn run_css(source: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    run_with_grammar(source, check, tree_sitter_css::LANGUAGE.into(), "t.css")
}

/// Run a tree-sitter `Check` against `source` parsed with the Rust
/// grammar.
#[must_use]
pub fn run_rust(source: &str, check: &dyn AstCheck) -> Vec<Diagnostic> {
    run_with_grammar(source, check, tree_sitter_rust::LANGUAGE.into(), "t.rs")
}

/// Same as `run_rust` but with a custom fake filename. Use this when
/// the rule filters on the file path (e.g. `src/main.rs` vs lib files).
#[must_use]
pub fn run_rust_with_path(source: &str, check: &dyn AstCheck, fake_path: &str) -> Vec<Diagnostic> {
    run_with_grammar(source, check, tree_sitter_rust::LANGUAGE.into(), fake_path)
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
    let tree = parser
        .parse(source, None)
        .expect("parser should produce a tree");
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
    let tree = parser
        .parse(source, None)
        .expect("parser should produce a tree");
    check.check(
        &CheckCtx::for_test_with_file(Path::new(fake_path), source, file),
        &tree,
    )
}

// ── oxc helpers ──────────────────────────────────────────────────────

use crate::rules::backend::OxcCheck;
use oxc_allocator::Allocator;
use oxc_parser::Parser as OxcParser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

/// Run an `OxcCheck` against `source` parsed as TypeScript (`.ts`).
#[must_use]
pub fn run_oxc_ts(source: &str, check: &dyn OxcCheck) -> Vec<Diagnostic> {
    run_oxc_with_source_type(source, check, SourceType::ts(), "t.ts")
}

/// Run an `OxcCheck` against `source` parsed as TSX (`.tsx`).
#[must_use]
pub fn run_oxc_tsx(source: &str, check: &dyn OxcCheck) -> Vec<Diagnostic> {
    run_oxc_with_source_type(source, check, SourceType::tsx(), "t.tsx")
}

/// Run an `OxcCheck` against `source` parsed as JavaScript (`.js`).
#[must_use]
pub fn run_oxc_js(source: &str, check: &dyn OxcCheck) -> Vec<Diagnostic> {
    run_oxc_with_source_type(source, check, SourceType::mjs(), "t.js")
}

/// Run an `OxcCheck` against `source` parsed as TypeScript with a custom path.
#[must_use]
pub fn run_oxc_ts_with_path(source: &str, check: &dyn OxcCheck, fake_path: &str) -> Vec<Diagnostic> {
    run_oxc_with_source_type(source, check, SourceType::ts(), fake_path)
}

/// Run an `OxcCheck` against `source` parsed as TSX with a custom path.
#[must_use]
pub fn run_oxc_tsx_with_path(
    source: &str,
    check: &dyn OxcCheck,
    fake_path: &str,
) -> Vec<Diagnostic> {
    run_oxc_with_source_type(source, check, SourceType::tsx(), fake_path)
}

/// Run an `OxcCheck` against `source` parsed as TypeScript with a
/// caller-supplied framework name. Seeds a `ProjectCtx` whose
/// `has_framework(name)` returns true.
#[must_use]
pub fn run_oxc_ts_with_framework(
    source: &str,
    check: &dyn OxcCheck,
    framework: &str,
) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    let project = ProjectCtx::for_test_with_framework(framework);
    let ctx = CheckCtx::for_test_with_project(Path::new("t.ts"), source, &project);

    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        return check.run_on_semantic(&semantic, &ctx);
    }

    let mut diagnostics = Vec::new();
    for node in semantic.nodes().iter() {
        let ty = node.kind().ty();
        if kinds.contains(&ty) {
            check.run(node, &ctx, &semantic, &mut diagnostics);
        }
    }
    diagnostics
}

/// Run an `OxcCheck` against `source` parsed as TSX with a
/// caller-supplied framework name.
#[must_use]
pub fn run_oxc_tsx_with_framework(
    source: &str,
    check: &dyn OxcCheck,
    framework: &str,
) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    let project = ProjectCtx::for_test_with_framework(framework);
    let ctx = CheckCtx::for_test_with_project(Path::new("t.tsx"), source, &project);

    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        return check.run_on_semantic(&semantic, &ctx);
    }

    let mut diagnostics = Vec::new();
    for node in semantic.nodes().iter() {
        let ty = node.kind().ty();
        if kinds.contains(&ty) {
            check.run(node, &ctx, &semantic, &mut diagnostics);
        }
    }
    diagnostics
}

/// Run an `OxcCheck` against `source` parsed as TSX with a
/// caller-supplied `ProjectCtx`.
#[must_use]
pub fn run_oxc_tsx_with_project(
    source: &str,
    check: &dyn OxcCheck,
    project: &ProjectCtx,
) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    let ctx = CheckCtx::for_test_with_project(Path::new("t.tsx"), source, project);

    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        return check.run_on_semantic(&semantic, &ctx);
    }

    let mut diagnostics = Vec::new();
    for node in semantic.nodes().iter() {
        let ty = node.kind().ty();
        if kinds.contains(&ty) {
            check.run(node, &ctx, &semantic, &mut diagnostics);
        }
    }
    diagnostics
}

/// Run an `OxcCheck` against `source` parsed as TypeScript with a custom path.
/// Seeds a `ProjectCtx` whose `has_framework(name)` returns true.
#[must_use]
pub fn run_oxc_ts_with_path_and_framework(
    source: &str,
    check: &dyn OxcCheck,
    fake_path: &str,
    framework: &str,
) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    let project = ProjectCtx::for_test_with_framework(framework);
    let ctx = CheckCtx::for_test_with_project(Path::new(fake_path), source, &project);

    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        return check.run_on_semantic(&semantic, &ctx);
    }

    let mut diagnostics = Vec::new();
    for node in semantic.nodes().iter() {
        let ty = node.kind().ty();
        if kinds.contains(&ty) {
            check.run(node, &ctx, &semantic, &mut diagnostics);
        }
    }
    diagnostics
}

/// Run an `OxcCheck` against `source` parsed as TSX with a custom path.
/// Seeds a `ProjectCtx` whose `has_framework(name)` returns true.
#[must_use]
pub fn run_oxc_tsx_with_path_and_framework(
    source: &str,
    check: &dyn OxcCheck,
    fake_path: &str,
    framework: &str,
) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    let project = ProjectCtx::for_test_with_framework(framework);
    let ctx = CheckCtx::for_test_with_project(Path::new(fake_path), source, &project);

    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        return check.run_on_semantic(&semantic, &ctx);
    }

    let mut diagnostics = Vec::new();
    for node in semantic.nodes().iter() {
        let ty = node.kind().ty();
        if kinds.contains(&ty) {
            check.run(node, &ctx, &semantic, &mut diagnostics);
        }
    }
    diagnostics
}

fn run_oxc_with_source_type(
    source: &str,
    check: &dyn OxcCheck,
    source_type: SourceType,
    fake_path: &str,
) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    let ctx = CheckCtx::for_test(Path::new(fake_path), source);

    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        return check.run_on_semantic(&semantic, &ctx);
    }

    let mut diagnostics = Vec::new();
    for node in semantic.nodes().iter() {
        let ty = node.kind().ty();
        if kinds.contains(&ty) {
            check.run(node, &ctx, &semantic, &mut diagnostics);
        }
    }
    diagnostics
}

/// Run an `OxcCheck` against `source` parsed as TSX with a caller-supplied
/// `FileCtx`. Use when the rule consults `ctx.file.path_segments` (e.g.
/// `in_test_dir`, `in_storybook`).
#[must_use]
pub fn run_oxc_tsx_with_file_ctx(
    source: &str,
    check: &dyn OxcCheck,
    file: &FileCtx,
) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    let ctx = CheckCtx::for_test_with_file(Path::new("t.tsx"), source, file);

    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        return check.run_on_semantic(&semantic, &ctx);
    }

    let mut diagnostics = Vec::new();
    for node in semantic.nodes().iter() {
        let ty = node.kind().ty();
        if kinds.contains(&ty) {
            check.run(node, &ctx, &semantic, &mut diagnostics);
        }
    }
    diagnostics
}
