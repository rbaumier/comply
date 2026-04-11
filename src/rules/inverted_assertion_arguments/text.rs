use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Check if a string looks like a literal (number, string, boolean, null, undefined).
fn is_literal(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    // String literals
    if (t.starts_with('"') && t.ends_with('"'))
        || (t.starts_with('\'') && t.ends_with('\''))
        || (t.starts_with('`') && t.ends_with('`'))
    {
        return true;
    }
    // Boolean / null / undefined
    if matches!(t, "true" | "false" | "null" | "undefined") {
        return true;
    }
    // Numeric literal (integer or float, possibly negative)
    let num = if let Some(rest) = t.strip_prefix('-') {
        rest
    } else {
        t
    };
    if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return true;
    }
    false
}

/// Check if a string looks like a variable/identifier (not a literal, not a call).
fn is_variable(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || is_literal(t) {
        return false;
    }
    // Not a function call or complex expression
    if t.contains('(') || t.contains(')') || t.contains(' ') {
        return false;
    }
    // Should start with a letter or underscore
    t.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
}

/// Extract the argument of `expect(...)` and `.toBe(...)`/`.toEqual(...)` from a line.
fn check_inverted(line: &str) -> bool {
    // Find expect(...)
    let Some(expect_start) = line.find("expect(") else {
        return false;
    };
    let after_expect = &line[expect_start + 7..];
    let expect_arg = extract_paren_content(after_expect);
    let expect_arg = match expect_arg {
        Some(a) => a,
        None => return false,
    };

    // Find .toBe(...) or .toEqual(...)
    let matchers = [".toBe(", ".toEqual("];
    for matcher in matchers {
        if let Some(pos) = line.find(matcher) {
            let after_matcher = &line[pos + matcher.len()..];
            if let Some(matcher_arg) = extract_paren_content(after_matcher) {
                if is_literal(expect_arg) && is_variable(matcher_arg) {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract content inside balanced parentheses, starting right after the `(`.
fn extract_paren_content(s: &str) -> Option<&str> {
    let mut depth = 1;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[..i]);
                }
            }
            _ => {}
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if check_inverted(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "inverted-assertion-arguments".into(),
                    message: "Expected and actual are inverted — put the literal in `.toBe()`/`.toEqual()`, not in `expect()`.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), source))
    }

    #[test]
    fn flags_literal_in_expect_variable_in_tobe() {
        assert_eq!(run(r#"  expect(42).toBe(result);"#).len(), 1);
    }

    #[test]
    fn flags_string_literal_in_expect() {
        assert_eq!(run(r#"  expect("hello").toEqual(name);"#).len(), 1);
    }

    #[test]
    fn allows_variable_in_expect_literal_in_tobe() {
        assert!(run(r#"  expect(result).toBe(42);"#).is_empty());
    }

    #[test]
    fn allows_variable_in_both() {
        assert!(run(r#"  expect(result).toBe(expected);"#).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("foo.ts"),
            r#"expect(42).toBe(result);"#,
        ));
        assert!(diags.is_empty());
    }
}
