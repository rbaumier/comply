use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if let Some(col) = find_function_star(lines[i]) {
                let start_line = i;
                // Find the opening brace
                let mut j = i;
                let mut found_open = false;
                while j < lines.len() {
                    if lines[j].contains('{') {
                        found_open = true;
                        break;
                    }
                    j += 1;
                }
                if !found_open {
                    i += 1;
                    continue;
                }
                // Track brace depth to find the matching close
                let mut depth: i32 = 0;
                let mut has_yield = false;
                let mut k = j;
                while k < lines.len() {
                    let line = lines[k];
                    for ch in line.chars() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    if contains_yield_keyword(line) {
                        has_yield = true;
                    }
                    if depth == 0 {
                        break;
                    }
                    k += 1;
                }
                if !has_yield {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: start_line + 1,
                        column: col + 1,
                        rule_id: "generator-without-yield".into(),
                        message: "Generator function does not contain a `yield` — add one or use a regular function.".into(),
                        severity: Severity::Warning,
                    });
                }
                i = k + 1;
            } else {
                i += 1;
            }
        }
        diagnostics
    }
}

/// Returns the column of `function*` if found, ignoring occurrences inside
/// strings or comments (basic heuristic).
fn find_function_star(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('*') {
        return None;
    }
    // Match `function*` or `function *`
    let patterns = ["function*", "function *"];
    for pat in &patterns {
        if let Some(pos) = line.find(pat) {
            return Some(pos);
        }
    }
    None
}

/// Check if a line contains the `yield` keyword (not inside a comment).
fn contains_yield_keyword(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return false;
    }
    // Look for `yield` as a keyword — preceded by whitespace/start, not part of
    // a larger identifier.
    let mut start = 0;
    while let Some(pos) = line[start..].find("yield") {
        let abs = start + pos;
        let before_ok = abs == 0 || !line.as_bytes()[abs - 1].is_ascii_alphanumeric();
        let after = abs + 5;
        let after_ok =
            after >= line.len() || !line.as_bytes()[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 5;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_generator_without_yield() {
        let src = "function* gen() {\n  return 42;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_generator_with_yield() {
        let src = "function* gen() {\n  yield 1;\n  yield 2;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_generator_with_space_star() {
        let src = "function *gen() {\n  console.log('nope');\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_regular_function() {
        let src = "function foo() {\n  return 1;\n}";
        assert!(run(src).is_empty());
    }
}
