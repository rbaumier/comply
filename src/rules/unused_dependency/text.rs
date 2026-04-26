//! unused-dependency detection — diff `package.json#dependencies` against the
//! set of bare specifiers the import index actually saw.
//!
//! Per-project guard: the check is project-wide, but the rule engine still
//! invokes us once per file. To emit each diagnostic exactly once we run only
//! when the current path is the lexicographically smallest indexed path —
//! a deterministic anchor that doesn't depend on filesystem iteration order.
//!
//! Skips:
//!   - `@types/*` packages — type definitions are consumed by the type
//!     checker, not by `import` statements.
//!   - Tooling packages used without explicit imports (`typescript`, `eslint`,
//!     `prettier`, `webpack`, `vite`, `turbo`, `jest`, `vitest`, `mocha`,
//!     `cypress`, `playwright`).
//!
//! Only inspects `dependencies` (production). `devDependencies`,
//! `peerDependencies`, and `optionalDependencies` are out of scope — they
//! have legitimate non-import use cases (CI scripts, type-only consumers,
//! optional native bindings).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const RULE_ID: &str = "unused-dependency";

/// Packages used by config files / CLI tooling rather than by `import`
/// statements. Adding to this list is conservative — false-positive flags
/// here are noisy, false-negatives just miss a few unused deps.
const TOOLING_ALLOWLIST: &[&str] = &[
    "typescript",
    "eslint",
    "prettier",
    "webpack",
    "vite",
    "turbo",
    "jest",
    "vitest",
    "mocha",
    "cypress",
    "playwright",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(pkg) = ctx.project.package_json.as_deref() else {
            return Vec::new();
        };

        let index = ctx.project.import_index();

        // Per-project guard: only fire on the lexicographically smallest
        // indexed path. Without this we'd emit the same N diagnostics once
        // per file in the project.
        let canon =
            std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());
        let anchor = index
            .iter_exports()
            .map(|(p, _)| p.to_path_buf())
            .min();
        match anchor {
            Some(a) if a == canon => {}
            _ => return Vec::new(),
        }

        let bare = index.bare_specifiers();
        let mut diagnostics = Vec::new();
        for dep in pkg.dependencies.keys() {
            if is_skipped(dep) {
                continue;
            }
            if bare.contains_key(dep) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: 1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "dependency `{dep}` is declared in package.json but never imported. \
                     Remove it, or add an import if it's actually needed."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn is_skipped(dep: &str) -> bool {
    if dep.starts_with("@types/") {
        return true;
    }
    TOOLING_ALLOWLIST.contains(&dep)
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

    fn run_on_project(
        files: &[(&str, &str)],
        package_json: &str,
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
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

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
        let ctx = CheckCtx {
            path: &target_path,
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    #[test]
    fn flags_unused_dep() {
        let pkg = r#"{
            "name": "demo",
            "dependencies": { "lodash": "^4.0.0" }
        }"#;
        let files: Vec<(&str, &str)> = vec![("a.ts", "export const x = 1;")];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert_eq!(diags.len(), 1, "lodash is declared but never imported");
        assert_eq!(diags[0].rule_id, "unused-dependency");
        assert!(
            diags[0].message.contains("lodash"),
            "message should name the unused dep, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_used_dep() {
        let pkg = r#"{
            "name": "demo",
            "dependencies": { "lodash": "^4.0.0" }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            ("a.ts", "import _ from 'lodash';\nexport const x = _;"),
        ];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "lodash is imported, no diagnostic expected, got: {diags:?}"
        );
    }

    #[test]
    fn skips_at_types_packages() {
        let pkg = r#"{
            "name": "demo",
            "dependencies": { "@types/node": "^20.0.0" }
        }"#;
        let files: Vec<(&str, &str)> = vec![("a.ts", "export const x = 1;")];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "@types/* packages must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn skips_tooling_packages() {
        let pkg = r#"{
            "name": "demo",
            "dependencies": {
                "typescript": "^5.0.0",
                "jest": "^29.0.0",
                "vite": "^5.0.0"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![("a.ts", "export const x = 1;")];
        let (_dir, diags) = run_on_project(&files, pkg, "a.ts");
        assert!(
            diags.is_empty(),
            "tooling packages must not be flagged, got: {diags:?}"
        );
    }
}
