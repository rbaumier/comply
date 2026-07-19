//! type-only-dependency detection — walk the project's `bare_specifiers`
//! map (built once during `ImportIndex` construction) and flag every
//! production dependency whose every importer uses `import type`.
//!
//! The check is project-scoped, but rules are dispatched per-file. To emit
//! diagnostics exactly once per run we gate execution on the first indexed
//! path in `ImportIndex::indexed_paths()` matching `ctx.path`. Subsequent
//! files re-enter the rule and short-circuit.
//!
//! Skips:
//!   - A published library (not `private`) that ships type declarations
//!     (`types`/`typings`/`exports`-types). Its `.d.ts` public surface can
//!     re-export a type-only dependency, so that dependency must resolve for
//!     downstream consumers and belongs in `dependencies`/`peerDependencies`.
//!   - `@types/*` packages — they're devDependencies by convention and exist
//!     to expose ambient types, so flagging them produces no actionable signal.
//!   - Packages absent from `dependencies` — already in `devDependencies` /
//!     `peerDependencies` / `optionalDependencies` / not declared at all.
//!     Only the dep → devDep migration is in scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const RULE_ID: &str = "type-only-dependency";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(pkg) = ctx.project.package_json.as_deref() else {
            return Vec::new();
        };

        // A published library (not `private`) that ships type declarations emits
        // a `.d.ts` whose public surface can re-export a type-only dependency;
        // that dependency must resolve for downstream consumers and belongs in
        // `dependencies`/`peerDependencies`, never `devDependencies`. This text
        // rule cannot prove which type-only imports escape into the exported
        // surface, so gate the whole suggestion off for such packages.
        if !pkg.is_private && pkg.ships_type_declarations {
            return Vec::new();
        }

        let index = ctx.project.import_index();
        let canon = index.canonical(ctx.path);
        let Some(anchor) = ctx.project.anchor_path() else {
            return Vec::new();
        };
        if anchor != canon {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (name, info) in index.bare_specifiers() {
            if !info.type_only {
                continue;
            }
            if name.starts_with("@types/") {
                continue;
            }
            if !pkg.dependencies.contains_key(name) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf().into(),
                line: 1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "package `{name}` is in `dependencies` but every import is `import type`. \
                     Move it to `devDependencies` — it's only needed at build time."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
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

    fn run_on_project(files: &[(&str, &str)], package_json: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), package_json).unwrap();

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

        let target_path: PathBuf = project
            .import_index()
            .indexed_paths()
            .min()
            .expect("at least one indexed file")
            .to_path_buf();
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx, lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    #[test]
    fn flags_prod_dep_used_only_via_import_type() {
        let pkg = r#"{
  "name": "demo",
  "dependencies": { "prisma-client": "1.0.0" }
}"#;
        let files = vec![(
            "app.ts",
            "import type { PrismaClient } from 'prisma-client';\nexport const x = 1;",
        )];
        let (_dir, diags) = run_on_project(&files, pkg);
        assert_eq!(diags.len(), 1, "prisma-client should be flagged");
        assert_eq!(diags[0].rule_id, RULE_ID);
        assert!(
            diags[0].message.contains("prisma-client"),
            "message should name the package, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_prod_dep_with_runtime_import() {
        let pkg = r#"{
  "name": "demo",
  "dependencies": { "prisma-client": "1.0.0" }
}"#;
        let files = vec![(
            "app.ts",
            "import { PrismaClient } from 'prisma-client';\nexport const x = new PrismaClient();",
        )];
        let (_dir, diags) = run_on_project(&files, pkg);
        assert!(
            diags.is_empty(),
            "runtime import means dep is needed at runtime"
        );
    }

    #[test]
    fn allows_dev_dep_even_if_type_only() {
        // Already in devDependencies — nothing to suggest.
        let pkg = r#"{
  "name": "demo",
  "devDependencies": { "prisma-client": "1.0.0" }
}"#;
        let files = vec![(
            "app.ts",
            "import type { PrismaClient } from 'prisma-client';\nexport const x = 1;",
        )];
        let (_dir, diags) = run_on_project(&files, pkg);
        assert!(diags.is_empty(), "devDep type-only is the desired state");
    }

    #[test]
    fn skips_types_packages() {
        // `@types/*` is meant to live in devDeps as types-only; flagging it
        // produces no actionable signal.
        let pkg = r#"{
  "name": "demo",
  "dependencies": { "@types/node": "20.0.0" }
}"#;
        let files = vec![(
            "app.ts",
            "import type { Buffer } from '@types/node';\nexport const x = 1;",
        )];
        let (_dir, diags) = run_on_project(&files, pkg);
        assert!(diags.is_empty(), "@types/* packages must not be flagged");
    }

    #[test]
    fn allows_published_library_shipping_type_declarations() {
        // A published package (`private: false`) that ships a `.d.ts` (`types`
        // field) may re-export a type-only dependency in its public surface, so
        // the dep must resolve for consumers and cannot move to devDependencies.
        let pkg = r#"{
  "name": "@refine/ui-types",
  "private": false,
  "types": "dist/index.d.ts",
  "dependencies": { "dayjs": "1.0.0" }
}"#;
        let files = vec![(
            "field.ts",
            "import type { ConfigType } from 'dayjs';\nexport type FieldProps<T = ConfigType> = { value: T };",
        )];
        let (_dir, diags) = run_on_project(&files, pkg);
        assert!(
            diags.is_empty(),
            "published lib shipping .d.ts must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn allows_published_library_with_exports_types_condition() {
        // Ships its `.d.ts` via an `exports` `types` condition rather than a
        // top-level `types` field — same shipped public surface, same gate.
        let pkg = r#"{
  "name": "@scope/lib",
  "exports": { ".": { "types": "./dist/index.d.ts", "import": "./dist/index.js" } },
  "dependencies": { "dayjs": "1.0.0" }
}"#;
        let files = vec![(
            "index.ts",
            "import type { ConfigType } from 'dayjs';\nexport type Stamp = ConfigType;",
        )];
        let (_dir, diags) = run_on_project(&files, pkg);
        assert!(
            diags.is_empty(),
            "exports `types` condition marks a shipped .d.ts, got: {diags:?}"
        );
    }

    #[test]
    fn flags_private_app_with_type_only_prod_dep() {
        // A private app bundles everything at build time; it ships no consumer
        // `.d.ts`, so its type-only prod dep is still flagged.
        let pkg = r#"{
  "name": "demo-app",
  "private": true,
  "types": "dist/index.d.ts",
  "dependencies": { "prisma-client": "1.0.0" }
}"#;
        let files = vec![(
            "app.ts",
            "import type { PrismaClient } from 'prisma-client';\nexport const x = 1;",
        )];
        let (_dir, diags) = run_on_project(&files, pkg);
        assert_eq!(diags.len(), 1, "private app's type-only prod dep still flagged");
    }

    #[test]
    fn flags_publishable_package_without_type_declarations() {
        // Publishable (`private` absent) but ships no `.d.ts`: `exports` has no
        // `types` condition and there is no `types`/`typings` field. The gate
        // requires shipping type declarations, so this is still flagged.
        let pkg = r#"{
  "name": "demo-lib",
  "exports": { ".": { "import": "./dist/index.js" } },
  "dependencies": { "prisma-client": "1.0.0" }
}"#;
        let files = vec![(
            "index.ts",
            "import type { PrismaClient } from 'prisma-client';\nexport const x = 1;",
        )];
        let (_dir, diags) = run_on_project(&files, pkg);
        assert_eq!(
            diags.len(),
            1,
            "publishable pkg shipping no .d.ts still flagged"
        );
    }
}
