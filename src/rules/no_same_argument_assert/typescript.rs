//! no-same-argument-assert backend — asserting a value equals itself.

use crate::diagnostic::{Diagnostic, Severity};

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
            if let Some(matcher_arg) = extract_paren_content(after_matcher)
                && expect_arg == matcher_arg.trim() {
                    return true;
                }
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    if !is_test_file(ctx.path) {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    for (idx, line) in text.lines().enumerate() {
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
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run_test_file(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), source), &tree)
    }

    #[test]
    fn flags_same_arg_tobe() {
        assert_eq!(run_test_file("  expect(x).toBe(x);").len(), 1);
    }

    #[test]
    fn flags_same_arg_to_equal() {
        assert_eq!(run_test_file("  expect(result).toEqual(result);").len(), 1);
    }

    #[test]
    fn allows_different_args() {
        assert!(run_test_file("  expect(actual).toBe(expected);").is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        // run_ts uses "t.ts" which is not a test file.
        assert!(crate::rules::test_helpers::run_ts("  expect(x).toBe(x);", &Check).is_empty());
    }
}
