//! playwright-no-page-pause text backend — flag `page.pause()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(col) = line.find(".pause()") {
                // Verify `page` precedes `.pause()` on the same line.
                let prefix = &line[..col];
                if prefix.contains("page") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: "playwright-no-page-pause".into(),
                        message: "`page.pause()` is a debug-only API — remove \
                                  before committing."
                            .into(),
                        severity: Severity::Error,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_page_pause() {
        let diags = run("login.test.ts", "await page.pause();");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-no-page-pause");
    }

    #[test]
    fn flags_page_pause_in_spec() {
        let diags = run("checkout.spec.ts", "  await page.pause();");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_other_pause() {
        // Not `page.pause()`.
        let diags = run("login.test.ts", "await video.pause();");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run("helpers.ts", "await page.pause();");
        assert!(diags.is_empty());
    }
}
