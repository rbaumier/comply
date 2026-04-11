use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Find assertion keywords in a substring.
fn has_assertion(text: &str) -> bool {
    text.contains("expect(")
        || text.contains("assert")
        || text.contains(".should")
        || text.contains(".toBe")
        || text.contains(".toEqual")
        || text.contains(".toMatch")
        || text.contains(".toThrow")
}

/// Extract test blocks and verify each contains an assertion.
/// Returns (line_number, test_name) for blocks missing assertions.
fn find_assertion_less_tests(source: &str) -> Vec<(usize, String)> {
    let mut results = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        // Match `it(` or `test(` at the start of a trimmed line
        let is_test = line.starts_with("it(") || line.starts_with("test(");
        if !is_test {
            i += 1;
            continue;
        }

        let test_line = i + 1; // 1-based
                               // Extract the test name from the opening quote
        let name = extract_test_name(line);

        // Find the block boundaries by counting braces
        let mut depth: i32 = 0;
        let mut found_open = false;
        let mut body = String::new();
        let mut j = i;
        while j < lines.len() {
            let l = lines[j];
            for ch in l.chars() {
                if ch == '{' {
                    depth += 1;
                    found_open = true;
                } else if ch == '}' {
                    depth -= 1;
                }
            }
            if j > i {
                body.push_str(l);
                body.push('\n');
            }
            if found_open && depth == 0 {
                break;
            }
            j += 1;
        }

        if !has_assertion(&body) {
            results.push((test_line, name));
        }

        i = j + 1;
    }

    results
}

fn extract_test_name(line: &str) -> String {
    // Try to extract from quotes
    for delim in ['"', '\'', '`'] {
        if let Some(start) = line.find(delim)
            && let Some(end) = line[start + 1..].find(delim) {
                return line[start + 1..start + 1 + end].to_string();
            }
    }
    "unnamed".to_string()
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        find_assertion_less_tests(ctx.source)
            .into_iter()
            .map(|(line, name)| Diagnostic {
                path: ctx.path.to_path_buf(),
                line,
                column: 1,
                rule_id: "assertions-in-tests".into(),
                message: format!("Test `{name}` has no assertion — add `expect(...)` or similar."),
                severity: Severity::Error,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), source))
    }

    fn run_non_test(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.ts"), source))
    }

    #[test]
    fn flags_test_without_assertion() {
        let src = r#"
test("should work", () => {
  const x = 1;
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_test_with_expect() {
        let src = r#"
test("should work", () => {
  expect(1).toBe(1);
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_it_block_without_assertion() {
        let src = r#"
it("does something", () => {
  const result = doThing();
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_test_with_assert() {
        let src = r#"
test("works", () => {
  assert.equal(a, b);
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let src = r#"
test("should work", () => {
  const x = 1;
});
"#;
        assert!(run_non_test(src).is_empty());
    }
}
