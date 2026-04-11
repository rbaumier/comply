use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Extract content inside balanced parentheses, starting right after `(`.
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

fn check_same_arg(line: &str) -> bool {
    let Some(expect_pos) = line.find("expect(") else {
        return false;
    };
    let after_expect = &line[expect_pos + 7..];
    let expect_arg = match extract_paren_content(after_expect) {
        Some(a) => a.trim(),
        None => return false,
    };

    if expect_arg.is_empty() {
        return false;
    }

    let matchers = [".toBe(", ".toEqual("];
    for matcher in matchers {
        if let Some(pos) = line.find(matcher) {
            let after_matcher = &line[pos + matcher.len()..];
            if let Some(matcher_arg) = extract_paren_content(after_matcher) {
                if expect_arg == matcher_arg.trim() {
                    return true;
                }
            }
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
            if check_same_arg(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-same-argument-assert".into(),
                    message:
                        "Asserting a value equals itself — this is always true and tests nothing."
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
    fn flags_same_arg_tobe() {
        assert_eq!(run("  expect(x).toBe(x);").len(), 1);
    }

    #[test]
    fn flags_same_arg_to_equal() {
        assert_eq!(run("  expect(result).toEqual(result);").len(), 1);
    }

    #[test]
    fn allows_different_args() {
        assert!(run("  expect(actual).toBe(expected);").is_empty());
    }

    #[test]
    fn allows_different_args_to_equal() {
        assert!(run("  expect(a).toEqual(b);").is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("foo.ts"),
            "  expect(x).toBe(x);",
        ));
        assert!(diags.is_empty());
    }
}
