//! generator-without-yield backend — flag generator functions missing `yield`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

fn find_function_star(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('*') {
        return None;
    }
    for pat in &["function*", "function *"] {
        if let Some(pos) = line.find(pat) {
            return Some(pos);
        }
    }
    None
}

fn contains_yield_keyword(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = line[start..].find("yield") {
        let abs = start + pos;
        let before_ok = abs == 0 || !line.as_bytes()[abs - 1].is_ascii_alphanumeric();
        let after = abs + 5;
        let after_ok = after >= line.len() || !line.as_bytes()[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 5;
    }
    false
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if let Some(col) = find_function_star(lines[i]) {
                let start_line = i;
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
                        message: "Generator function does not contain a `yield` — \
                                  add one or use a regular function."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_generator_without_yield() {
        let src = "function* gen() {\n  return 42;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_generator_with_yield() {
        let src = "function* gen() {\n  yield 1;\n  yield 2;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_regular_function() {
        let src = "function foo() {\n  return 1;\n}";
        assert!(run_on(src).is_empty());
    }
}
