//! vitest-no-commented-out-tests text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const COMMENT_PATTERNS: &[&str] = &[
    "// test(",
    "// test.skip(",
    "// it(",
    "// it.skip(",
    "// describe(",
    "// describe.skip(",
    "//test(",
    "//it(",
    "//describe(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.file.path_segments.in_test_dir
            && !ctx.path.to_string_lossy().contains(".test.")
            && !ctx.path.to_string_lossy().contains(".spec.")
        {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if COMMENT_PATTERNS.iter().any(|p| trimmed.starts_with(p)) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Commented-out test — delete it or restore behind `.skip`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), src))
    }

    #[test]
    fn flags_commented_test_call() {
        let src = "// test('does x', () => {});\nit('ok', () => {});";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_commented_describe() {
        let src = "//describe('section', () => {});";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_uncommented_test() {
        let src = "test('ok', () => {});";
        assert!(run(src).is_empty());
    }
}
