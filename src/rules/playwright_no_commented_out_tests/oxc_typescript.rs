//! playwright-no-commented-out-tests OXC backend — flag commented-out test blocks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Patterns that indicate a commented-out test or describe block.
fn looks_like_test_comment(text: &str) -> bool {
    let trimmed = text.trim_start();
    for kw in &["test", "it", "describe"] {
        if let Some(rest) = trimmed.strip_prefix(kw)
            && (rest.starts_with('(') || rest.starts_with('.') || rest.starts_with('['))
        {
            return true;
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        if !is_test_file(ctx.path) {
            return diagnostics;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return diagnostics;
        }

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(start..end) else {
                continue;
            };

            // Strip comment markers (// or /* */).
            let body = if let Some(rest) = raw.strip_prefix("//") {
                rest
            } else if let Some(rest) = raw.strip_prefix("/*").and_then(|r| r.strip_suffix("*/")) {
                rest
            } else {
                raw
            };

            for line in body.lines() {
                if looks_like_test_comment(line) {
                    let (line_num, column) = byte_offset_to_line_col(ctx.source, start);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: line_num,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Some tests seem to be commented.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(
            &format!("{PW_IMPORT}{source}"),
            &Check,
            "app.test.ts",
        )
    }

    #[test]
    fn flags_commented_test() {
        let d = run_ts("// test('should work', () => {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_commented_describe() {
        let d = run_ts("// describe('suite', () => {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_normal_comment() {
        let d = run_ts("// This is a normal comment");
        assert!(d.is_empty());
    }
}
