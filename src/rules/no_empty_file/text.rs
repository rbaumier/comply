use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if the source has no meaningful content — only whitespace,
/// ordinary comments, and `"use strict"` / `'use strict'` directives. Inner
/// doc comments (`//!`, `/*!`) count as meaningful content.
fn is_empty(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Inner doc comments (`//!`, `/*!`) document the enclosing module or
        // crate and become rustdoc output — a file of only these is a
        // deliberate documentation module, not empty. Checked before the
        // generic comment branches so it isn't swallowed by `starts_with("//")`.
        if trimmed.starts_with("//!") || trimmed.starts_with("/*!") {
            return false;
        }
        // Single-line comments
        if trimmed.starts_with("//") {
            continue;
        }
        // Block comment fragments
        if trimmed.starts_with("/*") || trimmed.starts_with('*') || trimmed.ends_with("*/") {
            continue;
        }
        // "use strict" directive
        if trimmed == r#""use strict";"#
            || trimmed == r#"'use strict';"#
            || trimmed == r#""use strict""#
            || trimmed == r#"'use strict'"#
        {
            continue;
        }
        // Triple-slash TS directives (e.g. `/// <reference ... />`)
        if trimmed.starts_with("///") {
            continue;
        }
        return false;
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Rust crate roots (lib.rs / main.rs) are legitimately empty
        // in CI-only packages and workspace stubs — Cargo requires the file.
        if ctx.lang == Language::Rust {
            let name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "lib.rs" || name == "main.rs" {
                return Vec::new();
            }
        } else {
            // An empty `index.{ts,tsx,js,jsx,mjs,cts,mts}` is the barrel/entry
            // placeholder convention: a package or workspace-project entry point
            // declared up front, meant to re-export (or be populated by the
            // build), and intentionally empty in source control. Exempting it
            // by the `index` stem covers package.json `main`/`exports` barrels
            // and tool-config entry points (e.g. Nx `project.json` `main`) alike
            // without special-casing any one build tool.
            let stem = ctx.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem == "index" {
                return Vec::new();
            }
        }
        if !is_empty(ctx.source) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "no-empty-file".into(),
            message: "File has no meaningful content — remove it or add code.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_empty_file() {
        assert_eq!(run("").len(), 1);
    }

    #[test]
    fn flags_whitespace_only() {
        assert_eq!(run("  \n\n  \n").len(), 1);
    }

    #[test]
    fn flags_comments_only() {
        assert_eq!(run("// this file is empty\n/* nothing */").len(), 1);
    }

    #[test]
    fn flags_use_strict_only() {
        assert_eq!(run("\"use strict\";\n").len(), 1);
    }

    #[test]
    fn allows_file_with_code() {
        assert!(run("export const x = 1;").is_empty());
    }

    #[test]
    fn allows_file_with_import() {
        assert!(run("import { foo } from './foo';").is_empty());
    }

    #[test]
    fn flags_triple_slash_only() {
        assert_eq!(run("/// <reference types=\"vite/client\" />").len(), 1);
    }

    #[test]
    fn empty_index_barrel_not_flagged() {
        // Issue #2285: an empty `index.ts` is an intentional barrel/entry
        // placeholder (Nx project entry, package barrel populated by the build).
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("modules/schematics/src/index.ts"),
            "",
        ));
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn empty_non_index_source_still_flagged() {
        // Negative space: an empty non-index source file has no entry role and
        // is a forgotten-file smell.
        let diags = Check.check(&CheckCtx::for_test(Path::new("src/service.ts"), ""));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn lib_rs_empty_not_flagged() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("lib.rs"), "// CI-only crate\n"));
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn main_rs_empty_not_flagged() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("main.rs"), ""));
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn other_rs_empty_still_flagged() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("utils.rs"), "// nothing\n"));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn inner_doc_comment_only_rs_not_flagged() {
        // Issue #3223: a Rust file of only `//!` inner doc comments (clap's
        // src/_features.rs) is a deliberate documentation module — rustdoc
        // output, re-exported as `pub mod _features;`, not empty.
        let source = "//! ## Documentation: Feature Flags\n//!\n//! Available feature flags.\n";
        let diags = Check.check(&CheckCtx::for_test(Path::new("_features.rs"), source));
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn inner_block_doc_comment_only_rs_not_flagged() {
        // The block form `/*! ... */` is also an inner doc comment.
        let source = "/*! module-level documentation */\n";
        let diags = Check.check(&CheckCtx::for_test(Path::new("_faq.rs"), source));
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn inner_doc_comment_plus_code_rs_not_flagged() {
        // A `//!` doc plus real code is unaffected (not empty, as before).
        let source = "//! module docs\npub fn answer() -> u8 { 42 }\n";
        let diags = Check.check(&CheckCtx::for_test(Path::new("module.rs"), source));
        assert_eq!(diags.len(), 0);
    }

    #[test]
    fn empty_eslint_fixture_in_test_dir_not_flagged() {
        // Issue #1091: empty fixture files in an ESLint-plugin test suite.
        let path =
            Path::new("common/tools/eslint-plugin-azure-sdk/tests/fixture/file.ts");
        let file = crate::rules::file_ctx::FileCtx::build(
            path,
            "",
            Language::TypeScript,
            crate::project::default_static_project_ctx(),
        );
        assert!(file.path_segments.in_test_dir);
        assert!(!crate::rules::no_empty_file::META.applies_to_file(&file));
    }

    #[test]
    fn empty_source_file_still_flagged() {
        // Control: an empty file outside any test directory is still a smell.
        let path = Path::new("src/empty.ts");
        let file = crate::rules::file_ctx::FileCtx::build(
            path,
            "",
            Language::TypeScript,
            crate::project::default_static_project_ctx(),
        );
        assert!(!file.path_segments.in_test_dir);
        assert!(crate::rules::no_empty_file::META.applies_to_file(&file));
        assert_eq!(Check.check(&CheckCtx::for_test(path, "")).len(), 1);
    }
}
