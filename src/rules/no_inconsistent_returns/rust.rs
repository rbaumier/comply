//! no-inconsistent-returns Rust backend.
//!
//! Flag functions that mix `return expr;` with bare `return;`.
//! Uses line-based scanning like the TS backend.

use crate::diagnostic::{Diagnostic, Severity};

fn is_function_head(trimmed: &str) -> bool {
    trimmed.starts_with("fn ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("pub(crate) fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub async fn ")
        || trimmed.starts_with("unsafe fn ")
        || trimmed.starts_with("pub unsafe fn ")
}

fn find_open_brace(lines: &[&str], start: usize) -> Option<usize> {
    (start..lines.len().min(start + 5)).find(|&i| lines[i].contains('{'))
}

fn find_matching_close(lines: &[&str], open_line: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    for (i, line) in lines.iter().enumerate().skip(open_line) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

fn scan_returns(lines: &[&str], start: usize, end: usize) -> (bool, bool) {
    let mut has_value_return = false;
    let mut has_bare_return = false;
    let mut depth: i32 = 0;
    let mut skip_depth: Option<i32> = None;

    for line in lines.iter().take(end + 1).skip(start) {
        let trimmed = line.trim();

        if skip_depth.is_none() && depth >= 1 && is_function_head(trimmed) {
            skip_depth = Some(depth);
        }

        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            }
        }

        if let Some(sd) = skip_depth {
            if depth <= sd {
                skip_depth = None;
            }
            continue;
        }

        if depth >= 1 {
            if trimmed == "return;" || trimmed == "return" {
                has_bare_return = true;
            } else if trimmed.starts_with("return ") || trimmed.starts_with("return\t") {
                has_value_return = true;
            }
        }
    }

    (has_value_return, has_bare_return)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if is_function_head(trimmed)
            && let Some(body_start) = find_open_brace(&lines, i)
            && let Some(body_end) = find_matching_close(&lines, body_start)
        {
            let (has_value_return, has_bare_return) =
                scan_returns(&lines, body_start, body_end);
            if has_value_return && has_bare_return {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: "no-inconsistent-returns".into(),
                    message: "Function has inconsistent returns \u{2014} some paths return a value, others return nothing.".into(),
                    severity: Severity::Warning,
                });
            }
            i = body_end + 1;
            continue;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_mixed_returns() {
        let code = r#"
fn foo(x: bool) -> Option<i32> {
    if x {
        return 42;
    }
    return;
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_consistent_value_returns() {
        let code = r#"
fn foo(x: bool) -> i32 {
    if x {
        return 42;
    }
    return 0;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_consistent_bare_returns() {
        let code = r#"
fn foo(x: bool) {
    if x {
        return;
    }
    return;
}
"#;
        assert!(run_on(code).is_empty());
    }
}
