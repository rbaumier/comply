//! require-path-exists OxcCheck backend — flag imports pointing to non-existent files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

const EXTENSIONS: &[&str] = &[
    "",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".mjs",
    ".cjs",
    ".json",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
    "/index.mjs",
];

fn is_relative_path(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

/// Resolve a path lexically — collapse `.`/`..` segments by string surgery
/// without touching the filesystem, since the target may not exist (the import
/// could point above the scanned tree). `..` pops the last normal segment;
/// a `..` with nothing left to pop is preserved so escaping the base stays
/// observable to the caller.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Whether the import resolves to a location comply can verify on disk: the
/// lexically-normalized resolved path must stay within `project_root`. An
/// import that escapes the root (e.g. `../../../../../shared.config.ts` reaching
/// above the checked-out tree in a monorepo/template layout) points outside the
/// scanned files, so its existence is unverifiable and must not be flagged.
/// When `project_root` is unknown, nothing is verifiable.
fn resolved_within_project(base_dir: &Path, import_spec: &str, project_root: &Path) -> bool {
    let resolved = normalize_lexical(&base_dir.join(import_spec));
    resolved.starts_with(normalize_lexical(project_root))
}

fn resolve_and_check(base_dir: &Path, import_spec: &str) -> bool {
    let resolved = base_dir.join(import_spec);

    for ext in EXTENSIONS {
        let candidate = if ext.is_empty() {
            resolved.clone()
        } else if let Some(dir_ext) = ext.strip_prefix('/') {
            resolved.join(dir_ext)
        } else if let Some(file_ext) = ext.strip_prefix('.') {
            resolved.with_extension(file_ext)
        } else {
            continue;
        };

        if candidate.exists() {
            return true;
        }
    }

    let with_ts = format!("{}.ts", resolved.display());
    let with_tsx = format!("{}.tsx", resolved.display());
    Path::new(&with_ts).exists() || Path::new(&with_tsx).exists()
}

fn extract_spec_from_string(source: &str, span: oxc_span::Span) -> &str {
    let raw = &source[span.start as usize..span.end as usize];
    raw.trim_matches(|c| c == '\'' || c == '"' || c == '`')
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ImportDeclaration,
            AstType::ExportNamedDeclaration,
            AstType::ExportDefaultDeclaration,
            AstType::CallExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let import_spec = match node.kind() {
            AstKind::ImportDeclaration(decl) => {
                extract_spec_from_string(ctx.source, decl.source.span).to_string()
            }
            AstKind::ExportNamedDeclaration(decl) => {
                let Some(ref src) = decl.source else { return };
                extract_spec_from_string(ctx.source, src.span).to_string()
            }
            AstKind::ExportDefaultDeclaration(_) => return,
            AstKind::CallExpression(call) => {
                // require("...")
                let is_require = match &call.callee {
                    oxc_ast::ast::Expression::Identifier(id) => id.name == "require",
                    _ => false,
                };
                if !is_require {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else { return };
                let oxc_ast::ast::Argument::StringLiteral(lit) = first_arg else { return };
                lit.value.to_string()
            }
            _ => return,
        };

        if !is_relative_path(&import_spec) {
            return;
        }

        if crate::rules::path_utils::is_generated_file_specifier(&import_spec) {
            return;
        }

        let Some(base_dir) = ctx.path.parent() else { return };

        // Only paths that stay within the project root are verifiable. An import
        // resolving above the root (or any path when the root is unknown) targets
        // files outside the scanned tree, so we cannot assert it is missing.
        let Some(project_root) = ctx.project.project_root.as_deref() else {
            return;
        };
        if !resolved_within_project(base_dir, &import_spec, project_root) {
            return;
        }

        if !resolve_and_check(base_dir, &import_spec) {
            let span = match node.kind() {
                AstKind::ImportDeclaration(d) => d.span,
                AstKind::ExportNamedDeclaration(d) => d.span,
                AstKind::CallExpression(c) => c.span,
                _ => return,
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Import path '{import_spec}' does not exist."),
                severity: Severity::Error,
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
        path: &Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use std::fs;
    use tempfile::TempDir;

    fn run_in_dir(importer_rel: &str, source: &str, on_disk: &[&str]) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        // A package.json anchors `project_root` at the TempDir root so the
        // escape check has a reference point (mirrors import-no-unresolved).
        fs::write(dir.path().join("package.json"), r#"{"name":"test"}"#).unwrap();
        for rel in on_disk {
            let p = dir.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, "export {};").unwrap();
        }
        let importer = dir.path().join(importer_rel);
        fs::create_dir_all(importer.parent().unwrap()).unwrap();
        fs::write(&importer, source).unwrap();
        let canon = fs::canonicalize(&importer).unwrap();
        let source_file = SourceFile {
            path: canon.clone(),
            language: Language::from_path(&canon).unwrap(),
        };
        let project = crate::project::ProjectCtx::load(&[&source_file], &Config::default());
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &canon,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn no_fp_for_prebuilt_build_output_issue_2065() {
        // astro reproducer: source imports a `.prebuilt.js` build artifact whose
        // only on-disk counterpart is the `.ts` source. The `.prebuilt.js` file
        // is generated by a separate build step and absent in a clean checkout,
        // so the import must not be flagged.
        let source =
            "import idle from '../../runtime/client/idle.prebuilt.js';";
        let diags = run_in_dir(
            "core/client-directive/default.ts",
            source,
            &["runtime/client/idle.ts"],
        );
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn flags_genuinely_missing_relative_import() {
        // A normal relative import to a file that does not exist on disk is a
        // real error (e.g. a typo'd path) and must still fire.
        let source = "import { x } from './does-not-exist';";
        let diags = run_in_dir("app.ts", source, &[]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("does-not-exist"));
    }

    #[test]
    fn no_fp_for_import_escaping_project_root_issue_1130() {
        // A monorepo/template import whose relative path resolves ABOVE the
        // project root (e.g. `sdk/.../arm-maps/vitest.esm.config.ts` importing
        // `../../../vitest.esm.shared.config.ts`, valid only at the Rush root)
        // targets a file outside the scanned tree. comply cannot verify it, so
        // it must not be flagged.
        let source = "import shared from '../../../../escapes.ts';";
        let diags = run_in_dir("sdk/pkg/config.ts", source, &[]);
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn flags_missing_parent_relative_import_within_root() {
        // A `../` import that stays UNDER the project root but points at a file
        // that does not exist is a genuine error and must still fire.
        let source = "import { x } from '../sibling/missing';";
        let diags = run_in_dir("sub/app.ts", source, &[]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing"));
    }

    #[test]
    fn allows_existing_parent_relative_import_within_root() {
        // A `../` import resolving to an existing file under the root is valid.
        let source = "import { x } from '../sibling/exists';";
        let diags = run_in_dir("sub/app.ts", source, &["sibling/exists.ts"]);
        assert!(diags.is_empty(), "got unexpected diagnostics: {diags:?}");
    }
}
