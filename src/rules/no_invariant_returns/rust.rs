//! no-invariant-returns Rust backend.
//!
//! Flag functions that always return the same literal value.

use crate::diagnostic::{Diagnostic, Severity};

fn extract_return_literal(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let after_return = trimmed.strip_prefix("return ")?;
    let value = after_return
        .strip_suffix(';')
        .unwrap_or(after_return)
        .trim();
    if value.is_empty() {
        return None;
    }
    if value == "true"
        || value == "false"
        || value == "None"
        || value.parse::<f64>().is_ok()
        || (value.starts_with('"') && value.ends_with('"'))
    {
        Some(value)
    } else {
        None
    }
}

fn is_function_head(trimmed: &str) -> bool {
    trimmed.starts_with("fn ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("pub(crate) fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub async fn ")
}

fn find_invariant_returns(source: &str) -> Vec<usize> {
    let lines: Vec<&str> = source.lines().collect();
    let mut flagged_lines: Vec<usize> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        let is_fn = is_function_head(trimmed);

        if is_fn && trimmed.contains('{') {
            let fn_start = i;
            let mut depth: i32 = 0;
            let mut returns: Vec<(usize, &str)> = Vec::new();
            let mut j = i;

            while j < lines.len() {
                for ch in lines[j].chars() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                if let Some(lit) = extract_return_literal(lines[j]) {
                    returns.push((j, lit));
                }
                if depth <= 0 && j > fn_start {
                    break;
                }
                j += 1;
            }

            if returns.len() >= 2 {
                let first = returns[0].1;
                if returns.iter().all(|(_, v)| *v == first) {
                    flagged_lines.push(fn_start);
                }
            }

            i = j + 1;
            continue;
        }
        i += 1;
    }

    flagged_lines
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for line_idx in find_invariant_returns(text) {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: line_idx + 1,
            column: 1,
            rule_id: "no-invariant-returns".into(),
            message: "Function always returns the same literal value \u{2014} consider using a constant instead.".into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_invariant_true() {
        let src = r#"
fn is_enabled(x: i32) -> bool {
    if x > 0 {
        return true;
    }
    return true;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_returns() {
        let src = r#"
fn is_positive(n: i32) -> bool {
    if n > 0 {
        return true;
    }
    return false;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_return() {
        let src = r#"
fn get_value() -> i32 {
    return 42;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
