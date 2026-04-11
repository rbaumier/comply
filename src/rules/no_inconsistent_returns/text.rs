use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Lightweight function-level scan: detect functions that mix
/// `return expr;` with bare `return;` (inconsistent returns).
impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let trimmed = lines[i].trim();
            // Detect function declarations / expressions
            if is_function_head(trimmed)
                && let Some(body_start) = find_open_brace(&lines, i)
                    && let Some(body_end) = find_matching_close(&lines, body_start) {
                        let (has_value_return, has_bare_return) =
                            scan_returns(&lines, body_start, body_end);
                        if has_value_return && has_bare_return {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: i + 1,
                                column: 1,
                                rule_id: "no-inconsistent-returns".into(),
                                message: "Function has inconsistent returns — some paths return a value, others return nothing.".into(),
                                severity: Severity::Warning,
                            });
                        }
                        // Skip past this function body to avoid re-scanning
                        i = body_end + 1;
                        continue;
                    }
            i += 1;
        }
        diagnostics
    }
}

/// Check if a line starts a function (fn, function, arrow with block body excluded here).
fn is_function_head(trimmed: &str) -> bool {
    // Rust: fn, JS/TS: function keyword, or method-like patterns
    trimmed.starts_with("function ")
        || trimmed.starts_with("function(")
        || trimmed.starts_with("async function ")
        || trimmed.starts_with("async function(")
        || trimmed.contains("function ") && trimmed.contains('(')
        || trimmed.starts_with("fn ")
}

/// Find the line index containing the opening `{` starting from `start`.
fn find_open_brace(lines: &[&str], start: usize) -> Option<usize> {
    (start..lines.len().min(start + 5)).find(|&i| lines[i].contains('{'))
}

/// Find the matching `}` for a `{` on `open_line`, counting brace depth.
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

/// Scan lines between open and close braces (exclusive of nested functions)
/// and return (has_value_return, has_bare_return).
fn scan_returns(lines: &[&str], start: usize, end: usize) -> (bool, bool) {
    let mut has_value_return = false;
    let mut has_bare_return = false;
    let mut depth: i32 = 0;
    let mut skip_depth: Option<i32> = None;

    for line in lines.iter().take(end + 1).skip(start) {
        let trimmed = line.trim();

        // Detect nested function — skip its entire body
        if skip_depth.is_none() && depth >= 1 && is_function_head(trimmed) {
            // Will start skipping once we see the opening brace
            skip_depth = Some(depth);
        }

        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            }
        }

        // If we're inside a nested function body, skip until we return to
        // the depth where the nested function was declared.
        if let Some(sd) = skip_depth {
            if depth <= sd {
                skip_depth = None;
            }
            continue;
        }

        // Inspect returns at any depth inside the outer function body (depth >= 1)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_mixed_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return 42;
    }
    return;
}
"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_consistent_value_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return 42;
    }
    return 0;
}
"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_consistent_bare_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return;
    }
    return;
}
"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn flags_async_function() {
        let code = r#"
async function fetchData(url) {
    if (!url) {
        return;
    }
    return fetch(url);
}
"#;
        assert_eq!(run(code).len(), 1);
    }
}
