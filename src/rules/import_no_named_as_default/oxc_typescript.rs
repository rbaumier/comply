//! import-no-named-as-default OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{CheckCtx, OxcCheck};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub struct Check;

/// Enumerated export surface of one source module, cached per import target.
struct SourceExports {
    /// Non-default export names (`export const foo`, `export { foo }`, …).
    named: FxHashSet<String>,
    /// Named bindings that are also the default export via
    /// `export { X as default }` — `import X from '…'` is valid for these.
    default_aliases: FxHashSet<String>,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let index = ctx.project.import_index();
        if index.is_empty() {
            return diagnostics;
        }

        let canon = index.canonical(ctx.path);

        // Per source module: the set of non-default export names, plus the set of
        // named bindings that are *also* the module's default export via
        // `export { X as default }`. `None` means "skip this source" (it has
        // `export * from '…'`, so its export surface can't be enumerated).
        let mut exports_by_source: FxHashMap<PathBuf, Option<SourceExports>> = FxHashMap::default();

        for imp in index.get_imports(&canon) {
            if imp.kind != ImportKind::Default {
                continue;
            }
            let Some(src) = &imp.source_path else {
                continue;
            };

            let summary = exports_by_source.entry(src.clone()).or_insert_with(|| {
                let exports = index.get_exports(src);
                if exports.iter().any(|e| e.kind == ExportKind::StarReExport) {
                    return None;
                }
                let named = exports
                    .iter()
                    .filter(|e| e.kind != ExportKind::Default)
                    .map(|e| e.name.clone())
                    .collect();
                // `export { X as default }` re-aliases the named binding `X` as
                // the default export — `import X from '…'` is then equivalent to
                // `import { X }`, not a mistake. Collect every such `X`.
                let default_aliases = exports
                    .iter()
                    .filter(|e| e.name == "default")
                    .filter_map(|e| e.local_name.clone())
                    .collect();
                Some(SourceExports {
                    named,
                    default_aliases,
                })
            });

            let Some(summary) = summary else {
                continue;
            };

            if summary.default_aliases.contains(&imp.local_name) {
                continue;
            }

            if summary.named.contains(&imp.local_name) {
                let (_line, _column) =
                    byte_offset_to_line_col(ctx.source, 0);
                // Use the import's line directly — it comes from the import index.
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: imp.line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{}` is a named export of `{}` — did you mean `import {{ {} }} from '{}'`?",
                        imp.local_name, imp.specifier, imp.local_name, imp.specifier
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> (TempDir, ProjectCtx, Vec<PathBuf>) {
        let dir = TempDir::new().unwrap();
        let mut source_files = Vec::new();
        let mut paths = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile { path: p.clone(), language: lang });
            paths.push(fs::canonicalize(&p).unwrap());
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::load(&refs, &Config::default());
        (dir, project, paths)
    }

    fn run(project: &ProjectCtx, path: &std::path::Path, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            path,
            project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn flags_default_import_matching_named_export() {
        let (_dir, project, paths) = setup_project(&[
            ("utils.ts", "export const foo = 1;\nexport default 42;"),
            ("app.ts", "import foo from './utils';"),
        ]);
        let diags = run(&project, &paths[1], "import foo from './utils';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn allows_default_aliased_from_named_export() {
        // Issue #1426: `export { Foo as default }` makes the default export the
        // same symbol as the named `Foo`, so `import Foo from '…'` is valid.
        let (_dir, project, paths) = setup_project(&[
            (
                "m.ts",
                "class Foo {}\nexport { Foo };\nexport { Foo as default };",
            ),
            ("app.ts", "import Foo from './m';"),
        ]);
        let diags = run(&project, &paths[1], "import Foo from './m';");
        assert!(diags.is_empty(), "expected no diagnostic, got {diags:?}");
    }

    #[test]
    fn allows_default_reexport_aliased_from_named() {
        // openai-node shape: a barrel re-exporting the same class both as a
        // named export and as the default via `export { X as default } from`.
        let (_dir, project, paths) = setup_project(&[
            ("client.ts", "export class OpenAI {}"),
            (
                "index.ts",
                "export { OpenAI } from './client';\n\
                 export { OpenAI as default } from './client';",
            ),
            ("test.ts", "import OpenAI from './index';"),
        ]);
        let diags = run(&project, &paths[2], "import OpenAI from './index';");
        assert!(diags.is_empty(), "expected no diagnostic, got {diags:?}");
    }

    #[test]
    fn flags_unrelated_default_with_coincidental_named() {
        // True positive: the default export is unrelated; a named `Foo` merely
        // coexists. `import Foo from '…'` is still the classic mistake.
        let (_dir, project, paths) = setup_project(&[
            (
                "m.ts",
                "export const Foo = 1;\nexport default function bar() {}",
            ),
            ("app.ts", "import Foo from './m';"),
        ]);
        let diags = run(&project, &paths[1], "import Foo from './m';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Foo"));
    }
}
