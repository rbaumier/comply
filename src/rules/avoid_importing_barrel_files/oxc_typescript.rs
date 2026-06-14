//! avoid-importing-barrel-files OXC backend — flag relative imports that
//! resolve to a genuine barrel file.
//!
//! A barrel is an `index` module whose exports are exclusively re-exports of
//! sibling modules (`export { x } from './x'`, `export * from './x'`):
//! importing it pulls in the whole subtree and defeats tree-shaking. An
//! `index` module that carries its own implementation (e.g. a function whose
//! sole source file happens to be `index.ts`) is not a barrel and is left
//! alone.
//!
//! When the cross-file import index is populated, the target is classified by
//! its exports. A pure re-export `index` is still left alone when it is the
//! only source file in its directory: that is the module organized across its
//! own subtree, not a hub over sibling modules. A target that is its own
//! package's declared entry point (`main` / `exports["."]`) is also left alone:
//! a relative import reaching another monorepo package's root `index` is the
//! intended cross-package consumption path, not a deep same-package barrel.
//! Without the index (no project context) the rule falls back to the filename
//! shape, treating any `index`-suffixed specifier as a barrel.
//!
//! Type-only imports (`import type { X } from '.'`, or a named import whose
//! every specifier carries an inline `type` qualifier) are erased at compile
//! time and carry no runtime barrel cost, so they are never flagged.
//!
//! Skipped when the importing file lives under a `routes/` directory: that's
//! the TanStack Router file-system convention where `index.tsx` is the leaf
//! route module for a segment, not a re-export hub.
//!
//! Skipped in test files: the rule's concern is production tree-shaking and
//! startup cost, which test files never ship into. Importing a component's
//! barrel from its `__tests__/` directory is the idiomatic way to exercise the
//! public API surface, not a barrel-file anti-pattern.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::import_index::{ExportKind, ExportedSymbol};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ImportDeclaration, ImportDeclarationSpecifier};
use std::path::Path;
use std::sync::Arc;

const INDEX_SUFFIXES: &[&str] = &[
    "/index",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
    "/index.mjs",
    "/index.cjs",
];

/// `true` when the specifier targets a directory or `index` module by shape —
/// the cheap pre-screen before any cross-file inspection.
fn is_barrel_path(module: &str) -> bool {
    if !module.starts_with('.') {
        return false;
    }
    if module == "." || module == ".." {
        return true;
    }
    if module.ends_with('/') {
        return true;
    }
    INDEX_SUFFIXES.iter().any(|s| module.ends_with(s))
}

/// `true` when the target's exports are exclusively re-exports of other
/// modules — the defining trait of a barrel. A file with at least one
/// own-implementation export (a declared `const`/`function`/`class`/`default`,
/// a `type`/`interface`) is a real module, not a barrel.
fn target_is_genuine_barrel(exports: &[ExportedSymbol]) -> bool {
    !exports.is_empty()
        && exports
            .iter()
            .all(|e| matches!(e.kind, ExportKind::ReExport | ExportKind::StarReExport))
}

fn is_tanstack_route_file(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == "routes")
}

/// Test files import a component's barrel to exercise its public API surface —
/// the idiomatic pattern in any component library test suite. The barrel cost
/// the rule guards against is a production-bundle concern; test files never
/// ship. Delegates to the shared path classifier so the exemption stays in sync
/// with every other rule's test-directory handling (`__tests__/`, `test/`,
/// `tests/`, `e2e/`, `.test.`/`.spec.` markers).
fn is_test_file(path: &Path) -> bool {
    crate::rules::file_ctx::scan_path(path).in_test_dir
}

/// `true` when the import has zero runtime impact: either a top-level
/// `import type { ... }` declaration, or a declaration where every named
/// specifier carries an inline `type` qualifier (`import { type A, type B }`).
/// Such imports are erased at compile time and pull nothing from the module
/// graph at runtime, so the barrel-file cost the rule guards against does not
/// apply.
fn is_type_only(import: &ImportDeclaration) -> bool {
    if import.import_kind.is_type() {
        return true;
    }
    let Some(specifiers) = &import.specifiers else {
        return false;
    };
    let mut saw_named = false;
    for spec in specifiers {
        match spec {
            ImportDeclarationSpecifier::ImportSpecifier(named) => {
                saw_named = true;
                if !named.import_kind.is_type() {
                    return false;
                }
            }
            // A default or namespace specifier is always a value binding.
            _ => return false,
        }
    }
    saw_named
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        // Type-only imports are erased at compile time and carry no runtime
        // barrel cost, so they are never flagged.
        if is_type_only(import) {
            return;
        }
        let module = import.source.value.as_str();
        if !is_barrel_path(module) {
            return;
        }
        if is_tanstack_route_file(ctx.path) {
            return;
        }
        if is_test_file(ctx.path) {
            return;
        }

        // With cross-file visibility, only flag targets that are actually
        // re-export hubs. Without it (no project context), fall back to the
        // filename shape.
        let index = ctx.project.import_index();
        if !index.is_empty() {
            let canon =
                std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());
            let resolved = index
                .get_imports(&canon)
                .iter()
                .find(|imp| imp.specifier == module)
                .and_then(|imp| imp.source_path.as_deref());
            let Some(target) = resolved else {
                return;
            };
            // A relative import that reaches another package's declared entry
            // point (`main` / `exports["."]`) is the intended cross-package
            // consumption path, not a deep same-package barrel: the entry `index`
            // is the package's public surface, with no direct alternative.
            if ctx.project.is_package_entry_file(target) {
                return;
            }
            if !target_is_genuine_barrel(index.get_exports(target)) {
                return;
            }
            // A pure re-export `index` that is the only source file in its
            // directory is the module itself organized across its own subtree,
            // not a hub over sibling modules — importing it pulls in exactly
            // that subtree.
            if index.is_sole_file_in_dir(target) {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from barrel file `{module}` — import directly from the source module instead."
            ),
            severity: Severity::Warning,
            span: Some((
                import.span.start as usize,
                (import.span.end - import.span.start) as usize,
            )),
        });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_explicit_index_import() {
        let d = run_on("import { foo } from './utils/index';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("barrel file"));
    }

    #[test]
    fn flags_explicit_index_with_extension() {
        let d = run_on("import { foo } from './utils/index.ts';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_directory_with_trailing_slash() {
        let d = run_on("import { foo } from './utils/';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_current_dir_import() {
        let d = run_on("import { foo } from '.';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_parent_dir_import() {
        let d = run_on("import { foo } from '..';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_file_import() {
        assert!(run_on("import { foo } from './utils/string';").is_empty());
    }

    #[test]
    fn allows_package_import() {
        assert!(run_on("import { useState } from 'react';").is_empty());
    }

    #[test]
    fn allows_file_named_index_like() {
        assert!(run_on("import { foo } from './indexer';").is_empty());
    }

    #[test]
    fn allows_index_import_from_tanstack_route_file() {
        // Regression for #160: TanStack route files (under `routes/`) commonly
        // import `./<segment>/index` as a leaf route module, not a barrel.
        let d = crate::rules::test_helpers::run_rule(&Check, "import { Route } from './_authed/index';", "src/routes/__root.tsx");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_type_only_import_from_current_dir() {
        // Regression for #1696: `import type { X } from '.'` is erased at
        // compile time and has zero runtime barrel cost.
        assert!(run_on("import type { SandpackFileExplorerProp } from '.';").is_empty());
    }

    #[test]
    fn allows_type_only_import_from_index() {
        assert!(run_on("import type { Foo } from './utils/index';").is_empty());
    }

    #[test]
    fn allows_inline_all_type_specifiers_from_barrel() {
        // `import { type A, type B }` — every specifier is type-only, so the
        // whole declaration is erased at runtime.
        assert!(run_on("import { type A, type B } from '.';").is_empty());
    }

    #[test]
    fn flags_value_import_from_barrel() {
        // Negative space: a genuine value import from a barrel is still flagged.
        let d = run_on("import { X } from '.';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("barrel file"));
    }

    #[test]
    fn allows_parent_barrel_import_from_test_file() {
        // Regression for #1537: a component's `__tests__/` file imports the
        // parent directory barrel (`..`) to exercise the public API surface —
        // the idiomatic component-library test pattern, not a barrel anti-pattern.
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "import Tabs from '..';",
            "components/tabs/__tests__/index.test.tsx",
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn still_flags_barrel_import_from_production_source() {
        // Negative space for #1537: a production source file importing from a
        // barrel must still fire — the exemption is keyed on the test-directory
        // convention, not relaxed for every file.
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "import Tabs from '..';",
            "components/tabs/index.tsx",
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got {d:?}");
        assert!(d[0].message.contains("barrel file"));
    }

    #[test]
    fn flags_mixed_import_with_value_specifier() {
        // At least one value specifier means the barrel is loaded at runtime.
        let d = run_on("import { type A, B } from '.';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("barrel file"));
    }

    #[test]
    fn flags_default_import_from_barrel() {
        // A default specifier is always a value binding, never type-only.
        let d = run_on("import Foo from '.';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn target_is_genuine_barrel_classification() {
        let reexport = ExportedSymbol {
            name: "foo".into(),
            kind: ExportKind::ReExport,
            line: 1,
            reexport_source: Some("./foo".into()),
            params: vec![],
            is_type_only: false,
            local_name: None,
        };
        let own = ExportedSymbol {
            name: "addBusinessDays".into(),
            kind: ExportKind::Named,
            line: 1,
            reexport_source: None,
            params: vec![],
            is_type_only: false,
            local_name: None,
        };
        // Pure re-export hub → barrel.
        assert!(target_is_genuine_barrel(&[reexport.clone()]));
        // Own implementation present → not a barrel.
        assert!(!target_is_genuine_barrel(&[own.clone()]));
        assert!(!target_is_genuine_barrel(&[reexport, own]));
        // No exports at all → not classifiable as a barrel.
        assert!(!target_is_genuine_barrel(&[]));
    }
}

#[cfg(test)]
mod cross_file_tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use std::path::PathBuf;
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
            source_files.push(SourceFile {
                path: p.clone(),
                language: lang,
            });
            paths.push(fs::canonicalize(&p).unwrap());
        }

        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        (dir, project, paths)
    }

    #[test]
    fn allows_index_that_is_sole_implementation() {
        // Regression for #1917: date-fns lays out each function as a directory
        // whose only source file is `index.ts` holding the implementation.
        // Importing it pulls in exactly that one module — it is not a barrel.
        let (_dir, project, paths) = setup_project(&[
            (
                "src/addBusinessDays/index.ts",
                "export function addBusinessDays(date: Date, amount: number): Date {\n  return date;\n}\n",
            ),
            (
                "test/addBusinessDays/basic.ts",
                "import { addBusinessDays } from '../../src/addBusinessDays/index.ts';\naddBusinessDays;",
            ),
        ]);
        let source =
            "import { addBusinessDays } from '../../src/addBusinessDays/index.ts';\naddBusinessDays;";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &paths[1],
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_js_specifier_index_that_is_sole_implementation() {
        // Regression for #1174: payloadcms lays out a module as a directory
        // whose only source file is `index.ts` holding the implementation, and
        // imports it via the emitted `.js` extension (`./seed/index.js`). The
        // `.js` specifier resolves to the `.ts` source; it carries real
        // implementation, not re-exports, so it is not a barrel.
        let (_dir, project, paths) = setup_project(&[
            (
                "test/plugin-redirects/seed/index.ts",
                "export const seed = async (): Promise<void> => {};\n",
            ),
            (
                "test/plugin-redirects/config.ts",
                "import { seed } from './seed/index.js';\nseed;",
            ),
        ]);
        let source = "import { seed } from './seed/index.js';\nseed;";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &paths[1],
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_sole_index_that_reexports_its_own_subtree() {
        // Regression for #1174: a directory whose only source file is `index.ts`
        // is the module itself, not a re-export hub — even when that `index.ts`
        // organizes its implementation across its OWN subdirectories via
        // re-exports. Importing it pulls in exactly that subtree, which is the
        // module. The content check alone would misclassify this pure-re-export
        // index as a barrel; the sole-file-in-directory shape rescues it.
        let (_dir, project, paths) = setup_project(&[
            ("src/Button/internal/impl.ts", "export const Button = 1;"),
            (
                "src/Button/index.ts",
                "export { Button } from './internal/impl.js';",
            ),
            (
                "src/app.ts",
                "import { Button } from './Button/index.js';\nButton;",
            ),
        ]);
        let source = "import { Button } from './Button/index.js';\nButton;";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &paths[2],
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn flags_genuine_reexport_barrel() {
        // A real barrel: `index.ts` only re-exports its siblings.
        let (_dir, project, paths) = setup_project(&[
            ("src/lib/a.ts", "export const a = 1;"),
            ("src/lib/b.ts", "export const b = 2;"),
            (
                "src/lib/index.ts",
                "export { a } from './a';\nexport { b } from './b';",
            ),
            (
                "src/app.ts",
                "import { a } from './lib/index.ts';\na;",
            ),
        ]);
        let source = "import { a } from './lib/index.ts';\na;";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &paths[3],
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
        assert!(diags[0].message.contains("barrel file"));
    }

    #[test]
    fn allows_cross_package_relative_import_to_package_root_index() {
        // Regression for #1599: in a monorepo, a test config imports a sibling
        // package via a relative path to that package's root `index.js`, which
        // the package's own `package.json` declares as its `exports["."]` entry
        // point. The entry `index` is the package's public surface, so the
        // cross-package relative reference must not be flagged.
        let (_dir, project, paths) = setup_project(&[
            (
                "packages/adapter-cloudflare/package.json",
                "{\n  \"name\": \"@sveltejs/adapter-cloudflare\",\n  \"exports\": { \".\": { \"import\": \"./index.js\" } }\n}\n",
            ),
            (
                "packages/adapter-cloudflare/index.js",
                "export { a } from './a.js';\nexport { b } from './b.js';\n",
            ),
            ("packages/adapter-cloudflare/a.js", "export const a = 1;"),
            ("packages/adapter-cloudflare/b.js", "export const b = 2;"),
            (
                "packages/adapter-cloudflare/test/apps/workers/svelte.config.js",
                "import adapter from '../../../index.js';\nadapter;",
            ),
        ]);
        let source = "import adapter from '../../../index.js';\nadapter;";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &paths[4],
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn flags_same_package_deep_barrel_when_package_entry_is_elsewhere() {
        // Negative space for #1599: a deep same-package barrel (`src/lib/index.ts`
        // re-exporting siblings) sitting beside a `package.json` whose declared
        // entry is a *different* file is still a genuine barrel and must fire —
        // the entry-point exemption must not over-exempt every index in a package.
        let (_dir, project, paths) = setup_project(&[
            (
                "package.json",
                "{\n  \"name\": \"pkg\",\n  \"exports\": { \".\": \"./index.js\" }\n}\n",
            ),
            ("index.js", "export const root = 1;"),
            ("src/lib/a.ts", "export const a = 1;"),
            ("src/lib/b.ts", "export const b = 2;"),
            (
                "src/lib/index.ts",
                "export { a } from './a';\nexport { b } from './b';",
            ),
            ("src/app.ts", "import { a } from './lib/index.ts';\na;"),
        ]);
        let source = "import { a } from './lib/index.ts';\na;";
        let diags = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &paths[5],
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        );
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
        assert!(diags[0].message.contains("barrel file"));
    }
}
