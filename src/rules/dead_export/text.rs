//! dead-export detection — walk every export in the current file and verify
//! it has at least one linked importer in the index.
//!
//! Skips:
//!   - Test files (`*.test.*`, `*.spec.*`, `tests/`, `__tests__/`) — these
//!     may legitimately export fixtures used only internally.
//!   - Entry points (`main.*`, `index.*` at the project root) — they are the
//!     consumer, not the consumed, and aren't imported by convention.
//!   - Star re-exports (`export * from './m'`) — the re-export doesn't carry
//!     a specific name to link against; it's a barrel, not a dead symbol.
//!   - Reusable UI library directories (`components/ui/`, `lib/ui/`) — these
//!     hold drop-in components (shadcn convention) that are installed for
//!     future use; flagging them every time a developer adds one before its
//!     first import is pure noise.
//!   - Generated files (containing a `// @generated` or `/* @generated */`
//!     header in the first ~40 lines) — code generators emit a fixed export
//!     surface that callers may pick from gradually.
//!
//! False-positive guards:
//!   - If any file imports the current module via a namespace import
//!     (`import * as ns from './m'`), `symbol_usages` is intentionally not
//!     populated for individual names. In that case every export on the
//!     module is treated as live — we can't tell from the index alone which
//!     specific names `ns.*` accesses touch.
//!   - `export default` is matched against the `"default"` usage key.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::path_utils::is_config_file;
use std::path::Path;

const RULE_ID: &str = "dead-export";

/// Path segments that mark a directory as a reusable UI component library.
/// Matched against the canonicalised path with forward-slash separators.
const UI_LIBRARY_DIRS: &[&str] = &["/components/ui/", "/lib/ui/", "/src/components/ui/"];

fn is_in_ui_library(path: &Path) -> bool {
    let normalised = path.to_string_lossy().replace('\\', "/");
    UI_LIBRARY_DIRS.iter().any(|seg| normalised.contains(seg))
}

/// True if the source carries a `@generated` marker in its leading comments.
/// Only scans the first 2KB to keep the cost bounded; generators always emit
/// the marker at the top of the file.
fn is_generated(source: &str) -> bool {
    let head = &source[..source.len().min(2048)];
    head.contains("@generated")
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        if is_config_file(ctx.path) {
            return Vec::new();
        }
        if is_entry_point(ctx.path, ctx.project.project_root.as_deref()) {
            return Vec::new();
        }
        if is_in_ui_library(ctx.path) {
            return Vec::new();
        }
        if is_generated(ctx.source) {
            return Vec::new();
        }

        let index = ctx.project.import_index();
        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

        // Files inside framework entry dirs are consumed by the framework itself
        // (e.g. TanStack Router routes, shadcn ui components) — their exports
        // are read by tooling, not by other application files in the index.
        let canon_str = canon.to_string_lossy();
        if ctx
            .project
            .framework_entry_dirs()
            .any(|dir| canon_str.contains(dir))
        {
            return Vec::new();
        }
        let exports = index.get_exports(&canon);
        if exports.is_empty() {
            return Vec::new();
        }

        // If any importer uses namespace-import form, treat every export as
        // live — the index doesn't track which properties of `ns.*` are read.
        let reached_via_namespace = index
            .get_imports_to(&canon)
            .iter()
            .any(|imp| imp.kind == ImportKind::Namespace);
        if reached_via_namespace {
            return Vec::new();
        }

        let magic: std::collections::HashSet<&str> =
            ctx.project.framework_magic_exports().collect();

        let mut diagnostics = Vec::new();
        for export in exports {
            if matches!(export.kind, ExportKind::StarReExport) {
                continue;
            }
            if magic.contains(export.name.as_str()) {
                continue;
            }
            if !index.get_usages(&canon, &export.name).is_empty() {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: export.line,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "export `{}` is never imported elsewhere in the project. \
                     Remove it or document why it's part of the public surface.",
                    export.name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// Entry points we deliberately never flag: `main.*` and `index.*` directly
/// at the project root. Nested `index.ts` files (e.g. barrel files in
/// feature folders) are expected to be imported and stay subject to the rule.
fn is_entry_point(path: &Path, project_root: Option<&Path>) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    if stem != "main" && stem != "index" {
        return false;
    }
    let Some(root) = project_root else {
        // No root detected (LSP / single-file) — err on the side of silence
        // for these conventional names.
        return true;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let canon_parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    canon_parent == canon_root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p,
                language: lang,
            });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    #[test]
    fn flags_export_with_no_importer() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.ts", "export function computeTax() {}"),
            ("other.ts", "export const y = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.ts");
        assert_eq!(diags.len(), 1, "computeTax is never imported");
        assert_eq!(diags[0].rule_id, "dead-export");
        assert!(
            diags[0].message.contains("computeTax"),
            "message should name the dead export, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_export_imported_elsewhere() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.ts", "export function computeTax() {}"),
            ("app.ts", "import { computeTax } from './tax';"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.ts");
        assert!(diags.is_empty(), "computeTax is imported, no diagnostic");
    }

    #[test]
    fn ignores_root_entry_points() {
        // `index.ts` at the project root acts as the entry — not flagged.
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export function bootstrap() {}"),
            ("other.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "index.ts");
        assert!(diags.is_empty(), "root index.ts must not be flagged");
    }

    #[test]
    fn ignores_test_files() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.test.ts", "export function fixture() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.test.ts");
        assert!(diags.is_empty(), "test files must not be flagged");
    }

    #[test]
    fn ignores_module_consumed_via_namespace_import() {
        // When `import * as ns from './m'` exists, individual symbol usages
        // are intentionally not linked; flagging every export would be noise.
        let files: Vec<(&str, &str)> = vec![
            ("m.ts", "export const a = 1; export const b = 2;"),
            ("app.ts", "import * as ns from './m';"),
        ];
        let (_dir, diags) = run_on_project(&files, "m.ts");
        assert!(diags.is_empty(), "namespace importer suppresses dead-export");
    }

    #[test]
    fn flags_multiple_dead_exports_independently() {
        let files: Vec<(&str, &str)> = vec![
            ("m.ts", "export const a = 1;\nexport const b = 2;"),
            ("app.ts", "import { a } from './m';"),
        ];
        let (_dir, diags) = run_on_project(&files, "m.ts");
        assert_eq!(diags.len(), 1, "only `b` should be flagged");
        assert!(diags[0].message.contains('b'));
    }

    #[test]
    fn ignores_components_ui_directory() {
        // shadcn convention: drop-in components installed before any importer
        // exists must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("components/ui/button.tsx", "export function Button() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "components/ui/button.tsx");
        assert!(
            diags.is_empty(),
            "components/ui/* should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_src_components_ui_directory() {
        let files: Vec<(&str, &str)> = vec![
            ("src/components/ui/card.tsx", "export function Card() {}"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/components/ui/card.tsx");
        assert!(
            diags.is_empty(),
            "src/components/ui/* should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_lib_ui_directory() {
        let files: Vec<(&str, &str)> = vec![
            ("lib/ui/avatar.tsx", "export function Avatar() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "lib/ui/avatar.tsx");
        assert!(diags.is_empty(), "lib/ui/* should be skipped: {diags:?}");
    }

    #[test]
    fn ignores_generated_files() {
        let files: Vec<(&str, &str)> = vec![
            (
                "schema.ts",
                "// @generated by codegen. do not edit.\nexport const TableA = {};",
            ),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "schema.ts");
        assert!(
            diags.is_empty(),
            "@generated files should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_block_comment_generated_marker() {
        let files: Vec<(&str, &str)> = vec![
            (
                "schema.ts",
                "/* @generated */\nexport const Settings = {};",
            ),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "schema.ts");
        assert!(
            diags.is_empty(),
            "/* @generated */ should be skipped: {diags:?}"
        );
    }

    #[test]
    fn still_flags_components_outside_ui_dir() {
        let files: Vec<(&str, &str)> = vec![
            ("components/feature/header.tsx", "export function Header() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "components/feature/header.tsx");
        assert_eq!(
            diags.len(),
            1,
            "components/<feature>/ should still be flagged"
        );
    }
}
