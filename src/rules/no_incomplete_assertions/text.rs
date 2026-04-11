use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Known matchers that complete an assertion chain.
const MATCHERS: &[&str] = &[
    ".toBe",
    ".toEqual",
    ".toMatch",
    ".toThrow",
    ".toContain",
    ".toBeTruthy",
    ".toBeFalsy",
    ".toBeNull",
    ".toBeUndefined",
    ".toBeDefined",
    ".toBeGreaterThan",
    ".toBeLessThan",
    ".toBeInstanceOf",
    ".toHaveBeenCalled",
    ".toHaveBeenCalledWith",
    ".toHaveLength",
    ".toHaveProperty",
    ".toMatchObject",
    ".toMatchSnapshot",
    ".toMatchInlineSnapshot",
    ".toStrictEqual",
    ".resolves",
    ".rejects",
    ".toBeCloseTo",
    ".toBeNaN",
];

/// Check if a line has an incomplete assertion: `expect(...)` without a matcher.
fn is_incomplete_assertion(line: &str) -> bool {
    let trimmed = line.trim();

    // Must contain expect(
    let Some(expect_pos) = trimmed.find("expect(") else {
        return false;
    };
    let after_expect = &trimmed[expect_pos + 7..];

    // Find the closing paren of expect(...)
    let mut depth = 1;
    let mut close_pos = None;
    for (i, ch) in after_expect.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close_pos = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let Some(cp) = close_pos else { return false };
    let after_close = after_expect[cp + 1..].trim();

    // `expect(x);` — nothing after closing paren except semicolon
    if after_close.is_empty() || after_close == ";" {
        return true;
    }

    // `expect(x).not;` — ends with .not and no further matcher
    if after_close == ".not;" || after_close == ".not" {
        return true;
    }

    // Check if there's a recognized matcher
    let remainder = &trimmed[expect_pos + 7 + cp + 1..];
    for matcher in MATCHERS {
        if remainder.contains(matcher) {
            return false;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_incomplete_assertion(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-incomplete-assertions".into(),
                    message: "Incomplete assertion — `expect()` without a matcher tests nothing."
                        .into(),
                    severity: Severity::Error,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), source))
    }

    #[test]
    fn flags_bare_expect() {
        assert_eq!(run("  expect(value);").len(), 1);
    }

    #[test]
    fn flags_expect_dot_not_semicolon() {
        assert_eq!(run("  expect(value).not;").len(), 1);
    }

    #[test]
    fn allows_expect_with_tobe() {
        assert!(run("  expect(value).toBe(true);").is_empty());
    }

    #[test]
    fn allows_expect_with_to_equal() {
        assert!(run("  expect(value).toEqual({ a: 1 });").is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let diags = Check.check(&CheckCtx::for_test(Path::new("foo.ts"), "  expect(value);"));
        assert!(diags.is_empty());
    }
}
