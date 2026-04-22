//! jsdoc/require-file-overview — every source file should carry a
//! `@file` / `@fileoverview` / `@overview` JSDoc tag.
//!
//! A one-line file-overview comment is the cheapest possible onboarding
//! aid: anyone opening the file knows what they're looking at before
//! they read any code. Test files are exempted — they rarely benefit
//! from a "what this file does" banner because the title of the test
//! suite already says it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::jsdoc_helpers::scan_blocks;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Test files are excluded — the test suite name describes the
        // file well enough, and an @file banner per test is noise.
        if is_test_file(ctx.path) {
            return Vec::new();
        }

        let has_overview = scan_blocks(ctx.source).iter().any(|block| {
            block.tags().iter().any(|t| {
                matches!(t.name.as_str(), "file" | "fileoverview" | "overview")
            })
        });
        if has_overview {
            return Vec::new();
        }
        if ctx.source.trim().is_empty() {
            return Vec::new();
        }
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message:
                "File is missing a top-of-file JSDoc with `@file` / `@fileoverview` — add a one-line summary of what this file does."
                    .into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

fn is_test_file(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    let name_lower = name.to_ascii_lowercase();
    name_lower.contains(".test.")
        || name_lower.contains(".spec.")
        || name_lower.ends_with(".d.ts")
        || path
            .components()
            .any(|c| matches!(c.as_os_str().to_str(), Some("__tests__") | Some("tests") | Some("test")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_at(path: &str, src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), src))
    }

    #[test]
    fn flags_missing_overview() {
        let src = "export const x = 1;\n";
        assert_eq!(run_at("src/foo.ts", src).len(), 1);
    }

    #[test]
    fn allows_file_overview_tag() {
        let src = "/** @file what this file does. */\nexport const x = 1;\n";
        assert!(run_at("src/foo.ts", src).is_empty());
    }

    #[test]
    fn allows_fileoverview_tag() {
        let src = "/**\n * @fileoverview helpers.\n */\nexport const x = 1;\n";
        assert!(run_at("src/foo.ts", src).is_empty());
    }

    #[test]
    fn skips_test_files() {
        let src = "export const x = 1;\n";
        assert!(run_at("src/foo.test.ts", src).is_empty());
        assert!(run_at("src/foo.spec.ts", src).is_empty());
        assert!(run_at("src/__tests__/foo.ts", src).is_empty());
    }

    #[test]
    fn skips_declaration_files() {
        let src = "export const x: number;\n";
        assert!(run_at("src/foo.d.ts", src).is_empty());
    }

    #[test]
    fn skips_empty_file() {
        assert!(run_at("src/foo.ts", "   \n").is_empty());
    }
}
