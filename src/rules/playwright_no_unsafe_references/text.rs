//! playwright-no-unsafe-references text backend — flag `page.evaluate()` with a
//! single function argument that likely captures outer-scope variables.

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
            if let Some(col) = line.find("page.evaluate(") {
                let rest = &line[col + "page.evaluate(".len()..];
                // Heuristic: if the evaluate call contains an arrow function
                // or `function` and the line does NOT contain a comma after
                // the function body start (no second argument), flag it.
                let has_arrow = rest.contains("=>");

                // Heuristic: if there's an arrow function and no
                // top-level comma (no second argument), flag it.
                if has_arrow && !has_top_level_comma(rest) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: "playwright-no-unsafe-references".into(),
                        message: "`page.evaluate()` with a single function \
                                  argument — pass captured variables as the \
                                  second argument."
                            .into(),
                        severity: Severity::Warning,
                    });
                }
            }
        }
        diagnostics
    }
}

/// Check if there is a comma at the top-level of parentheses (depth 0 relative
/// to the opening paren of `evaluate(`). This indicates a second argument.
fn has_top_level_comma(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut string_char = ' ';

    for (i, ch) in s.char_indices() {
        if in_string {
            if ch == string_char && (i == 0 || s.as_bytes()[i - 1] != b'\\') {
                in_string = false;
            }
            continue;
        }

        match ch {
            '\'' | '"' | '`' => {
                in_string = true;
                string_char = ch;
            }
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            ',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_evaluate_with_single_arrow() {
        let diags = run(
            "login.test.ts",
            "await page.evaluate(() => document.title);",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-no-unsafe-references");
    }

    #[test]
    fn flags_evaluate_with_arrow_body() {
        let diags = run(
            "login.test.ts",
            "await page.evaluate(() => { return window.scrollY; });",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_evaluate_with_second_arg() {
        let diags = run(
            "login.test.ts",
            "await page.evaluate((name) => document.title + name, userName);",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_evaluate_with_string_arg() {
        let diags = run(
            "login.test.ts",
            "await page.evaluate('document.title');",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run(
            "helpers.ts",
            "await page.evaluate(() => document.title);",
        );
        assert!(diags.is_empty());
    }
}
