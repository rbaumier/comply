//! no-common-grab-bag backend — flag a `common` / `utils` / `helpers` /
//! `shared` / `misc` / `util` module once its public surface has accreted.
//!
//! The finding is the file itself — its name against its whole exported
//! surface — so the diagnostic is file-scoped and reports `1:1` by
//! declaration, not because a position was left uncomputed.
//!
//! Such a name describes nothing the module holds. A reader must open the file
//! to learn its contents. That cost is real when many unrelated symbols sit
//! behind the name. It is absent when the module holds a few helpers for its
//! own parent module.
//!
//! Accretion is what the anti-pattern is made of, so the rule measures it
//! rather than inferring it from the name. The symbol count comes from the
//! cross-file index (`ProjectCtx::import_index`), which derives it from each
//! file's AST: exported declarations for TS/JS and Vue, every `pub` item for
//! Rust — including the methods of an inherent `impl`. A wildcard re-export
//! (`export * from …`, `pub use …::*`) counts as one symbol, so a barrel built
//! only from wildcards stays silent. A module below `min_exports`
//! (defaults.toml) is not reported. Without an index (LSP single-file mode)
//! the rule stays silent, the same trade-off `god-module` makes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const RULE_ID: &str = "no-common-grab-bag";

const BANNED_STEMS: &[&str] = &["common", "utils", "helpers", "shared", "misc", "util"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(stem) = ctx.path.file_stem().and_then(|s| s.to_str()) else {
            return vec![];
        };
        if !BANNED_STEMS.iter().any(|banned| stem.eq_ignore_ascii_case(banned)) {
            return vec![];
        }

        let index = ctx.project.import_index();
        if index.is_empty() {
            return vec![];
        }

        let exported = index.get_exports(ctx.path).len();
        if exported < ctx.config.threshold(RULE_ID, "min_exports", ctx.lang) {
            return vec![];
        }

        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: RULE_ID.into(),
            message: format!(
                "exposes {exported} symbols to other modules behind a name that \
                 describes none of them. Split it into modules named after what \
                 each one owns."
            ),
            severity: Severity::Error,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Write `files` to a temp dir, index them, then run the rule on
    /// `target_rel`.
    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            let language = Language::from_path(&path).unwrap();
            source_files.push(SourceFile { path, language });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::for_test_with_files(&refs);

        let target = dir.path().join(target_rel);
        let source = fs::read_to_string(&target).unwrap();
        let diags = Check.check(&CheckCtx::for_test_with_project(&target, &source, &project));
        (dir, diags)
    }

    /// `count` TypeScript exports, one per line.
    fn ts_exports(count: usize) -> String {
        (0..count)
            .map(|i| format!("export function fn{i}() {{ return {i}; }}\n"))
            .collect()
    }

    /// `count` public Rust functions.
    fn rust_pub_fns(count: usize) -> String {
        (0..count)
            .map(|i| format!("pub fn fn{i}() -> usize {{ {i} }}\n"))
            .collect()
    }

    #[test]
    fn flags_grab_bag_inside_domain_specific_directory() {
        // vitejs/vite `packages/vite/src/node/utils.ts`: 118 exports under a
        // domain-specific parent (`node/`). The parent narrows nothing — the
        // module is a genuine bag and must stay flagged.
        let path = "packages/vite/src/node/utils.ts";
        let source = ts_exports(118);
        let (_dir, diags) = run_on_project(&[(path, &source)], path);
        assert_eq!(
            diags.len(),
            1,
            "a domain-specific parent directory does not shrink an accreted surface"
        );
    }

    #[test]
    fn allows_scoped_rust_helpers_module() {
        // astral-sh/ruff `crates/ruff_linter/src/rules/flake8_boolean_trap/helpers.rs`:
        // three helpers for one lint family. Closes #6877.
        let path = "crates/ruff_linter/src/rules/flake8_boolean_trap/helpers.rs";
        let source = rust_pub_fns(3);
        let (_dir, diags) = run_on_project(&[(path, &source)], path);
        assert!(
            diags.is_empty(),
            "three scoped helpers are not a grab-bag, got {diags:?}"
        );
    }

    #[test]
    fn flags_crate_wide_rust_helpers_module() {
        // astral-sh/ruff `crates/ruff_python_ast/src/helpers.rs`: 50 public
        // items, the crate's miscellaneous namespace.
        let path = "crates/ruff_python_ast/src/helpers.rs";
        let source = rust_pub_fns(40);
        let (_dir, diags) = run_on_project(&[(path, &source)], path);
        assert_eq!(
            diags.len(),
            1,
            "40 public items are a grab-bag, got {diags:?}"
        );
    }

    #[test]
    fn threshold_is_inclusive_at_min_exports() {
        let min = Config::default().threshold(RULE_ID, "min_exports", Language::TypeScript);
        let below = ts_exports(min.saturating_sub(1));
        let (_dir, quiet) = run_on_project(&[("src/utils.ts", &below)], "src/utils.ts");
        assert!(quiet.is_empty(), "one export short of {min} stays silent");

        let at = ts_exports(min);
        let (_dir, loud) = run_on_project(&[("src/utils.ts", &at)], "src/utils.ts");
        assert_eq!(loud.len(), 1, "{min} exports must be reported");
    }

    #[test]
    fn flags_every_banned_stem() {
        let source = ts_exports(40);
        for stem in BANNED_STEMS {
            let rel = format!("src/{stem}.ts");
            let (_dir, diags) = run_on_project(&[(&rel, &source)], &rel);
            assert_eq!(diags.len(), 1, "{rel} must be flagged");
            assert_eq!(diags[0].rule_id, RULE_ID);
        }
    }

    #[test]
    fn flags_commonjs_grab_bag() {
        // CommonJS exports reach the index through a different branch than ESM
        // ones, and a hand-written `utils.js` is where they show up.
        let source: String = (0..40)
            .map(|i| format!("exports.fn{i} = () => {i};\n"))
            .collect();
        let (_dir, diags) = run_on_project(&[("src/utils.js", &source)], "src/utils.js");
        assert_eq!(
            diags.len(),
            1,
            "40 CommonJS exports are a grab-bag, got {diags:?}"
        );
    }

    #[test]
    fn stays_silent_without_a_project_index() {
        // LSP single-file mode: no index, so the surface is unknown rather than
        // small. Reporting on the stem alone is what #6877 rejected.
        let source = ts_exports(40);
        let diags = Check.check(&CheckCtx::for_test(Path::new("src/utils.ts"), &source));
        assert!(diags.is_empty(), "no index means no signal, got {diags:?}");
    }

    #[test]
    fn allows_meaningful_names() {
        for path in ["src/order_service.ts", "src/payment.ts", "src/auth.rs"] {
            let source = if path.ends_with(".rs") {
                rust_pub_fns(40)
            } else {
                ts_exports(40)
            };
            let (_dir, diags) = run_on_project(&[(path, &source)], path);
            assert!(diags.is_empty(), "{path} should be allowed");
        }
    }

    #[test]
    fn allows_single_export_utils_file() {
        let files = [(
            "src/utils.ts",
            "import { clsx } from 'clsx';\n\
             export function cn(...args) { return clsx(args); }\n",
        )];
        let (_dir, diags) = run_on_project(&files, "src/utils.ts");
        assert!(diags.is_empty(), "one-helper utils file is not a bag");
    }
}
