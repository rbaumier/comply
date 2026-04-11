use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract the literal value from a `return <literal>;` statement, if any.
/// Returns `Some(literal_str)` for values like `return true;`, `return 0;`,
/// `return "hello";`, `return null;`.
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
    // Accept simple literals: booleans, null, undefined, numbers, quoted strings
    if value == "true"
        || value == "false"
        || value == "null"
        || value == "undefined"
        || value.parse::<f64>().is_ok()
        || (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        Some(value)
    } else {
        None
    }
}

/// Scan function bodies for invariant returns.
/// We look for `function` or arrow `=>` blocks, collect all `return` literals,
/// and flag when there are 2+ returns all returning the same literal.
fn find_invariant_returns(source: &str) -> Vec<usize> {
    let lines: Vec<&str> = source.lines().collect();
    let mut flagged_lines: Vec<usize> = Vec::new();

    // Track function start lines and their return values.
    // Simple heuristic: track brace depth from function declarations.
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        let is_fn = trimmed.contains("function ")
            || trimmed.contains("function(")
            || (trimmed.contains("=>") && trimmed.contains('{'));

        if is_fn && trimmed.contains('{') {
            let fn_start = i;
            let mut depth: i32 = 0;
            let mut returns: Vec<(usize, &str)> = Vec::new();
            let mut j = i;

            // Count braces from the function line
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
                    // Flag the function definition line
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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        find_invariant_returns(ctx.source)
            .into_iter()
            .map(|line_idx| Diagnostic {
                path: ctx.path.to_path_buf(),
                line: line_idx + 1,
                column: 1,
                rule_id: "no-invariant-returns".into(),
                message: "Function always returns the same literal value \u{2014} consider using a constant instead.".into(),
                severity: Severity::Warning,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_invariant_true() {
        let src = r#"
function isEnabled(x) {
    if (x > 0) {
        return true;
    }
    return true;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_invariant_number() {
        let src = r#"
function getDefault(mode) {
    if (mode === "a") {
        return 0;
    }
    return 0;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_different_returns() {
        let src = r#"
function isPositive(n) {
    if (n > 0) {
        return true;
    }
    return false;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_return() {
        let src = r#"
function getValue() {
    return 42;
}
"#;
        assert!(run(src).is_empty());
    }
}
