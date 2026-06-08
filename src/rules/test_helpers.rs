//! Unified test harness for comply rules.
//!
//! Each rule's backend file implements `RunRule` once; test call sites call
//! `run_rule(&Check, src, "t.ts")` without knowing whether the backend is
//! tree-sitter or OXC.
//!
//! # Entry points
//!
//! - [`run_rule`] — default project + file context, language from path
//! - [`run_rule_with_ctx`] — explicit project + file context
//! - [`run_rule_gated`] — applies the production `applies_to_file` gate
//! - [`run_rule_by_id`] — look up rule by string ID (for integration tests)

#![cfg(test)]

use std::path::Path;
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::{Language, SourceFile};
use crate::project::ProjectCtx;
use crate::rules::RuleDef;
use crate::rules::backend::{AstCheck, Backend, CheckCtx, OxcCheck};
use crate::rules::file_ctx::FileCtx;
use crate::rules::meta::RuleMeta;
use oxc_allocator::Allocator;
use oxc_parser::Parser as OxcParser;
use oxc_semantic::SemanticBuilder;

// ── RunRule unified entry points ──────────────────────────────────────

/// Backend-agnostic test-dispatch trait. Each rule's backend file implements
/// this once; test call sites use `run_rule(&Check, src, "t.ts")` without
/// knowing whether the backend is tree-sitter or oxc.
pub trait RunRule: Send + Sync {
    fn meta(&self) -> &'static RuleMeta;
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &Path,
        project: &ProjectCtx,
        file: &FileCtx,
    ) -> Vec<Diagnostic>;
}

/// Internal: run a tree-sitter AstCheck with explicit context.
pub(crate) fn run_ast_check(
    check: &dyn AstCheck,
    src: &str,
    path: &Path,
    project: &ProjectCtx,
    file: &FileCtx,
) -> Vec<Diagnostic> {
    let lang = Language::from_path(path).unwrap_or(Language::TypeScript);
    let mut parser = tree_sitter::Parser::new();
    let Some(tree) = crate::parsing::parse_with_grammar(&mut parser, lang, src.as_bytes()) else {
        return vec![];
    };
    check.check(&CheckCtx::for_test_full(path, src, project, file), &tree)
}

/// Internal: run an OxcCheck with explicit context.
pub(crate) fn run_oxc_check(
    check: &dyn OxcCheck,
    src: &str,
    path: &Path,
    project: &ProjectCtx,
    file: &FileCtx,
) -> Vec<Diagnostic> {
    crate::oxc_helpers::reset_file_caches();
    let source_type = crate::oxc_helpers::source_type_for_path(path);
    let allocator = Allocator::default();
    let parse_ret = OxcParser::new(&allocator, src, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    let ctx = CheckCtx::for_test_full(path, src, project, file);
    let kinds = check.interested_kinds();
    if kinds.is_empty() {
        return check.run_on_semantic(&semantic, &ctx);
    }
    let mut diagnostics = Vec::new();
    for node in semantic.nodes().iter() {
        if kinds.contains(&node.kind().ty()) {
            check.run(node, &ctx, &semantic, &mut diagnostics);
        }
    }
    diagnostics
}

/// Run a rule against `src`, using the language inferred from `path`.
/// Equivalent to the old `run_ts` / `run_oxc_ts` without explicit context.
#[must_use]
pub fn run_rule(check: &dyn RunRule, src: &str, path: impl AsRef<Path>) -> Vec<Diagnostic> {
    check.execute_with_ctx(
        src,
        path.as_ref(),
        crate::project::default_static_project_ctx(),
        crate::rules::file_ctx::default_static_file_ctx(),
    )
}

/// Run a rule with explicit project and file context.
#[must_use]
pub fn run_rule_with_ctx(
    check: &dyn RunRule,
    src: &str,
    path: impl AsRef<Path>,
    project: &ProjectCtx,
    file: &FileCtx,
) -> Vec<Diagnostic> {
    check.execute_with_ctx(src, path.as_ref(), project, file)
}

/// Run a rule through the production applicability gate (`applies_to_file`).
/// Returns `[]` when the rule would be skipped for the given path (e.g., when
/// `meta.skip_in_test_dir = true` and the path is inside `__tests__/`).
#[must_use]
pub fn run_rule_gated(check: &dyn RunRule, src: &str, path: impl AsRef<Path>) -> Vec<Diagnostic> {
    let path = path.as_ref();
    let lang = Language::from_path(path).unwrap_or(Language::TypeScript);
    let project = crate::project::default_static_project_ctx();
    let file = FileCtx::build(path, src, lang, project);
    if !check.meta().applies_to_file(&file) {
        return vec![];
    }
    check.execute_with_ctx(src, path, project, &file)
}

/// Run a rule looked up by its string ID. Uses default project + file context.
/// Returns `[]` for backends that don't support in-process dispatch (Oxlint,
/// Clippy, Tsc, Tsgolint, TypeAware).
#[must_use]
pub fn run_rule_by_id(id: &str, src: &str, path: impl AsRef<Path>) -> Vec<Diagnostic> {
    let path = path.as_ref();
    let lang = Language::from_path(path).unwrap_or(Language::TypeScript);
    let rule = crate::rules::all_rule_defs_static()
        .iter()
        .find(|r| r.meta.id == id)
        .unwrap_or_else(|| panic!("rule not found: {id}"));
    let Some((_, backend)) = rule.backends.iter().find(|(l, _)| *l == lang) else {
        return vec![];
    };
    let project = crate::project::default_static_project_ctx();
    let file = crate::rules::file_ctx::default_static_file_ctx();
    match backend {
        Backend::TreeSitter(check) => run_ast_check(check.as_ref(), src, path, project, file),
        Backend::Oxc(check) => run_oxc_check(check.as_ref(), src, path, project, file),
        Backend::Text(check) => check.check(&CheckCtx::for_test(path, src)),
        _ => vec![],
    }
}

// ── Retained utility helpers ───────────────────────────────────────────

/// Build a temporary K8s project from in-memory YAML fixtures.
/// Returns `(tempdir, project, paths)` — drop `tempdir` last.
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
        files.push(SourceFile { path, language: Language::Yaml });
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `run_rule_gated` must honour `skip_in_test_dir`.
    /// A rule with `skip_in_test_dir = true` should return no diagnostics for
    /// paths inside `__tests__/`, while the same rule does fire on normal paths.
    #[test]
    fn run_rule_gated_suppresses_in_test_dir() {
        // tailwind-no-arbitrary-z-index has skip_in_test_dir = true and a TSX backend.
        let src = r#"const X = () => <div className="z-[999]">hi</div>;"#;
        let rule = crate::rules::all_rule_defs_static()
            .iter()
            .find(|r| r.meta.id == "tailwind-no-arbitrary-z-index")
            .expect("rule should exist");
        // Verify the gate: applies_to_file must be false for __tests__/ paths.
        let lang = Language::Tsx;
        let project = crate::project::default_static_project_ctx();
        let test_file = FileCtx::build(
            std::path::Path::new("__tests__/page.tsx"),
            src,
            lang,
            project,
        );
        let normal_file = FileCtx::build(
            std::path::Path::new("app/page.tsx"),
            src,
            lang,
            project,
        );
        assert!(
            rule.meta.applies_to_file(&normal_file),
            "rule must apply on a normal path"
        );
        assert!(
            !rule.meta.applies_to_file(&test_file),
            "skip_in_test_dir must suppress the rule inside __tests__/"
        );
    }
}

