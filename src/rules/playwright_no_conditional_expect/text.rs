//! playwright-no-conditional-expect text backend — flag `expect()` inside conditionals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Keywords that open a conditional block.
const CONDITIONAL_OPENERS: &[&str] = &["if ", "if(", "switch ", "switch(", "catch ", "catch("];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        // Track brace depth separately for conditionals.
        // When we see a conditional opener, record the current brace depth.
        // Any `expect(` at a deeper brace depth is inside a conditional.
        let mut diagnostics = Vec::new();
        let mut brace_depth: i32 = 0;
        // Stack of brace depths at which conditionals were opened.
        let mut conditional_starts: Vec<i32> = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Detect conditional openers before counting braces on this line.
            let is_conditional = CONDITIONAL_OPENERS
                .iter()
                .any(|kw| trimmed.starts_with(kw))
                || trimmed.contains("catch(")
                || trimmed.contains("catch ");
            let is_else = trimmed.starts_with("} else {")
                || trimmed.starts_with("} else if")
                || trimmed.starts_with("else {")
                || trimmed.contains("else {")
                || trimmed.contains("else if");

            // Process each character to track braces and detect expect().
            let mut expect_col = None;
            for (ci, ch) in line.char_indices() {
                match ch {
                    '{' => {
                        if brace_depth == 0 || is_conditional || is_else {
                            // Only push if this is the opening brace of a conditional.
                            if is_conditional || is_else {
                                conditional_starts.push(brace_depth);
                            }
                        }
                        brace_depth += 1;
                    }
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth < 0 {
                            brace_depth = 0;
                        }
                        // Pop conditional starts that match this depth.
                        while conditional_starts.last() == Some(&brace_depth) {
                            conditional_starts.pop();
                        }
                    }
                    'e' if expect_col.is_none() => {
                        // Check if this is the start of `expect(`.
                        if line[ci..].starts_with("expect(") {
                            expect_col = Some(ci);
                        }
                    }
                    _ => {}
                }
            }

            // If we found an `expect(` and we're inside a conditional, flag it.
            if let Some(col) = expect_col
                && !conditional_starts.is_empty()
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "playwright-no-conditional-expect".into(),
                    message: "`expect()` inside a conditional may silently \
                              skip — assert unconditionally."
                        .into(),
                    severity: Severity::Warning,
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

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_expect_inside_if() {
        let source = "\
if (condition) {
  expect(value).toBe(true);
}";
        let diags = run("login.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-no-conditional-expect");
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn flags_expect_inside_catch() {
        let source = "\
try {
  doSomething();
} catch(e) {
  expect(e.message).toBe('error');
}";
        let diags = run("error.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
    }

    #[test]
    fn allows_expect_at_top_level() {
        let source = "expect(value).toBe(true);";
        let diags = run("login.test.ts", source);
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let source = "\
if (condition) {
  expect(value).toBe(true);
}";
        let diags = run("helpers.ts", source);
        assert!(diags.is_empty());
    }
}
