//! testing-no-mocking-internal-modules OXC backend — detect `vi.mock`/`jest.mock`
//! calls whose first argument is a relative path (`./` or `../`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Check;

/// Module source extensions a test file's subject can live under, ordered by
/// the resolution preference TypeScript/Vitest applies for a bare relative
/// specifier (`./foo` → `foo.ts` before `foo.js`).
const SUBJECT_EXTENSIONS: &[&str] =
    &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

fn unquote(raw: &str) -> &str {
    raw.trim_start_matches(['\'', '"', '`'])
        .trim_end_matches(['\'', '"', '`'])
}

/// Lexically resolve `specifier` (a relative module path like `../astro`)
/// against `base_dir`, normalising `.`/`..` segments and dropping any module
/// extension, so two specifiers that name the same module from different
/// directories compare equal. No filesystem access — purely textual.
fn resolve_module_key(base_dir: &Path, specifier: &str) -> PathBuf {
    let mut resolved = base_dir.to_path_buf();
    for part in specifier.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }
    strip_module_extension(&resolved)
}

/// Drop a trailing module extension (`foo.ts` → `foo`) so a mock of `../astro`
/// and an import of `./astro.ts` resolve to the same key. Leaves a path without
/// a known module extension untouched.
fn strip_module_extension(path: &Path) -> PathBuf {
    let has_module_ext = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| SUBJECT_EXTENSIONS.contains(&ext));
    if has_module_ext {
        path.with_extension("")
    } else {
        path.to_path_buf()
    }
}

/// Filesystem locations the module under test for `test_path` may live at: the
/// sibling of the same stem (`foo.spec.ts` → `foo.ts`), and — when the test
/// sits in a conventional test directory (`__tests__`, `test`, …) — the
/// same-stem file in the parent directory. Returns the resolved subject path
/// plus the directory imports inside it are relative to.
fn subject_candidates(test_path: &Path) -> Vec<PathBuf> {
    let Some(test_dir) = test_path.parent() else {
        return Vec::new();
    };
    let Some(stem) = subject_stem(test_path) else {
        return Vec::new();
    };

    let mut dirs = vec![test_dir.to_path_buf()];
    if is_test_directory(test_dir)
        && let Some(parent) = test_dir.parent()
    {
        dirs.push(parent.to_path_buf());
    }

    let mut candidates = Vec::new();
    for dir in dirs {
        for ext in SUBJECT_EXTENSIONS {
            let candidate = dir.join(format!("{stem}.{ext}"));
            if candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

/// The stem of the module a test file targets: `longRunningApps.test.ts`,
/// `longRunningApps.spec.tsx` → `longRunningApps`. Returns `None` when the file
/// is not a recognisable `.test`/`.spec` test file.
fn subject_stem(test_path: &Path) -> Option<&str> {
    let stem = test_path.file_stem()?.to_str()?;
    stem.strip_suffix(".test")
        .or_else(|| stem.strip_suffix(".spec"))
}

/// True when `dir`'s final segment is a conventional unit-test directory, so a
/// test inside it targets a module one level up.
fn is_test_directory(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            matches!(name, "__tests__" | "__test__" | "test" | "tests" | "__mocks__")
        })
}

/// Set of module keys the subject module directly imports via a relative
/// specifier, each resolved against the subject's own directory. Bare
/// (package) specifiers are ignored — only relative `./`/`../` imports define
/// the module's internal dependency surface.
fn subject_relative_import_keys(subject_path: &Path) -> Vec<PathBuf> {
    let Some(subject_dir) = subject_path.parent() else {
        return Vec::new();
    };
    let Ok(source) = std::fs::read_to_string(subject_path) else {
        return Vec::new();
    };
    relative_specifiers(&source)
        .into_iter()
        .map(|spec| resolve_module_key(subject_dir, &spec))
        .collect()
}

/// Relative module specifiers (`./x`, `../y`) referenced by `source`, covering
/// `import`/`export … from`, `require(...)` and dynamic `import(...)`. A
/// lightweight textual scan keyed on the quote that follows the leading `.`,
/// sufficient to recognise a module's declared dependencies.
fn relative_specifiers(source: &str) -> Vec<String> {
    let mut specs = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let quote = bytes[i];
        if (quote == b'\'' || quote == b'"' || quote == b'`')
            && let Some(end) = source[i + 1..].find(quote as char)
        {
            let inner = &source[i + 1..i + 1 + end];
            if inner.starts_with("./") || inner.starts_with("../") {
                specs.push(inner.to_string());
            }
            i += end + 2;
            continue;
        }
        i += 1;
    }
    specs
}

/// True when mocking `mock_path` (relative to `test_path`'s directory) targets
/// a direct relative import of the test file's subject module — the legitimate
/// "isolate the unit under test from its own dependencies" pattern, which must
/// not be flagged.
fn mocks_subjects_direct_dependency(test_path: &Path, mock_path: &str) -> bool {
    let Some(test_dir) = test_path.parent() else {
        return false;
    };
    let mock_key = resolve_module_key(test_dir, mock_path);
    subject_candidates(test_path)
        .iter()
        .flat_map(|subject| subject_relative_import_keys(subject))
        .any(|import_key| import_key == mock_key)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["jest.mock", "vi.mock"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // `test/internal/` tests deliberately exercise internal implementation
        // details, so mocking internal modules there is intentional (unlike
        // `test/public/` tests that verify the public API contract).
        if ctx.file.path_segments.in_test_internal_dir {
            return;
        }

        // Callee must be vi.mock or jest.mock
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "mock" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else { return };
        let obj_name = obj.name.as_str();
        if obj_name != "vi" && obj_name != "jest" {
            return;
        }

        // First argument must be a string literal starting with "./" or "../"
        let Some(first_arg) = call.arguments.first() else { return };
        let raw = &ctx.source[first_arg.span().start as usize..first_arg.span().end as usize];
        let path = unquote(raw);

        if path.starts_with("./") || path.starts_with("../") {
            // Mocking a module's own direct dependency to isolate the unit
            // under test is the intended unit-testing pattern, not coupling to
            // hidden internals — don't flag it.
            if mocks_subjects_direct_dependency(ctx.path, path) {
                return;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, first_arg.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Mocking internal module '{path}' couples tests to implementation details — mock boundaries, not internals."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Language;
    use crate::rules::file_ctx::FileCtx;

    fn run(s: &str, path: &str) -> Vec<Diagnostic> {
        let lang = Language::from_path(std::path::Path::new(path)).unwrap_or(Language::TypeScript);
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(std::path::Path::new(path), s, lang, project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, s, path, project, &file)
    }

    #[test]
    fn flags_vi_mock_relative_in_plain_test_file() {
        assert_eq!(run("vi.mock('../utils/helpers');", "test/foo.spec.ts").len(), 1);
    }

    #[test]
    fn flags_vi_mock_relative_in_public_test_dir() {
        // `test/public/` verifies the public API contract — still flagged.
        assert_eq!(run("vi.mock('../../src/service.js');", "test/public/foo.spec.ts").len(), 1);
    }

    #[test]
    fn allows_mocking_external_package() {
        assert!(run("vi.mock('axios');", "test/foo.spec.ts").is_empty());
    }

    #[test]
    fn allows_internal_mock_in_test_internal_dir_issue1150() {
        // `test/internal/` tests deliberately mock internal modules. (Closes #1150)
        assert!(
            run(
                "vi.mock('../../../src/util/userAgentPlatform.js', async () => ({}));",
                "sdk/core/core-rest-pipeline/test/internal/node/userAgent.spec.ts"
            )
            .is_empty()
        );
    }

    /// Run the check against an on-disk test file so the subject module can be
    /// read from the filesystem.
    fn run_on_disk(test_path: &std::path::Path) -> Vec<Diagnostic> {
        let src = std::fs::read_to_string(test_path).unwrap();
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(test_path, &src, Language::TypeScript, project);
        crate::rules::test_helpers::run_oxc_check(&Check, &src, test_path, project, &file)
    }

    #[test]
    fn allows_mocking_subjects_direct_dependency_issue1803() {
        // clerk/javascript: a test for `longRunningApps.ts` mocks the preset
        // modules that module directly imports, to isolate the unit under test
        // from heavy framework configs. Mocking a module's own declared
        // dependencies is the intended unit-testing pattern. (Closes #1803)
        let dir = tempfile::tempdir().unwrap();
        let presets = dir.path().join("integration").join("presets");
        let tests_dir = presets.join("__tests__");
        std::fs::create_dir_all(&tests_dir).unwrap();

        std::fs::write(
            presets.join("longRunningApps.ts"),
            "import { astro } from './astro';\n\
             import { expo } from './expo';\n\
             import { express } from './express';\n\
             import { next } from './next';\n\
             export const createLongRunningApps = () => ({ astro, expo, express, next });\n",
        )
        .unwrap();
        for preset in ["astro", "expo", "express", "next"] {
            std::fs::write(presets.join(format!("{preset}.ts")), "export const x = 1;\n")
                .unwrap();
        }

        let test_path = tests_dir.join("longRunningApps.test.ts");
        std::fs::write(
            &test_path,
            "import { describe, expect, it, vi } from 'vitest';\n\
             const deepProxy = (): any => new Proxy({}, { get: () => ({}) });\n\
             vi.mock('../astro', () => ({ astro: deepProxy() }));\n\
             vi.mock('../expo', () => ({ expo: deepProxy() }));\n\
             vi.mock('../express', () => ({ express: deepProxy() }));\n\
             vi.mock('../next', () => ({ next: deepProxy() }));\n\
             describe('createLongRunningApps', () => { it('works', () => {}); });\n",
        )
        .unwrap();

        assert!(
            run_on_disk(&test_path).is_empty(),
            "mocking the subject module's direct dependencies must not be flagged"
        );
    }

    #[test]
    fn flags_mock_not_imported_by_subject_issue1803() {
        // A relative mock of a module the subject does NOT import is genuine
        // internal-implementation coupling and stays flagged.
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        std::fs::write(
            src_dir.join("widget.ts"),
            "import { config } from './config';\nexport const widget = () => config;\n",
        )
        .unwrap();
        std::fs::write(src_dir.join("config.ts"), "export const config = 1;\n").unwrap();
        std::fs::write(
            src_dir.join("secretHelper.ts"),
            "export const secretHelper = () => 1;\n",
        )
        .unwrap();

        let test_path = src_dir.join("widget.test.ts");
        std::fs::write(
            &test_path,
            "import { vi } from 'vitest';\nvi.mock('./secretHelper');\n",
        )
        .unwrap();

        assert_eq!(
            run_on_disk(&test_path).len(),
            1,
            "mocking a module the subject does not import is still coupling and must be flagged"
        );
    }
}
