//! prefer-default-parameters — flag `x = x || 'default'` / `x = x ?? 'default'` in function bodies.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Check whether `line` matches `IDENT = IDENT (|| | ??) LITERAL ;`
/// where both IDENTs are the same name. Returns the column of the match start.
fn find_default_reassignment(line: &str) -> Option<usize> {
    // Look for `||` or `??` operator
    for op in ["||", "??"] {
        let Some(op_pos) = line.find(op) else {
            continue;
        };

        // Left side: `IDENT = IDENT`
        let left = line[..op_pos].trim_end();
        // Split at `=`
        let Some(eq_pos) = left.rfind('=') else {
            continue;
        };
        // Make sure it's `=` not `==`, `!=`, `<=`, `>=`
        if eq_pos > 0 && matches!(left.as_bytes()[eq_pos - 1], b'!' | b'<' | b'>' | b'=') {
            continue;
        }
        if left.len() > eq_pos + 1 && left.as_bytes()[eq_pos + 1] == b'=' {
            continue;
        }

        let lhs = left[..eq_pos].trim();
        let rhs_ident = left[eq_pos + 1..].trim();

        // Both must be plain identifiers and equal
        if lhs.is_empty()
            || rhs_ident.is_empty()
            || lhs != rhs_ident
            || !lhs.bytes().all(is_ident_char)
        {
            continue;
        }

        // Right side of `||`/`??`: must be a literal (string, number, boolean, null, undefined)
        let rhs_val = line[op_pos + op.len()..]
            .trim()
            .trim_end_matches(';')
            .trim();
        if rhs_val.is_empty() {
            continue;
        }
        if is_literal_value(rhs_val) {
            // Column of the LHS identifier
            let col = line.find(lhs).unwrap_or(0);
            return Some(col);
        }
    }
    None
}

/// Check if a value looks like a JS literal: string, number, boolean, null, undefined.
fn is_literal_value(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // String literal (single, double, backtick)
    let first = s.as_bytes()[0];
    let last = s.as_bytes()[s.len() - 1];
    if matches!(first, b'\'' | b'"' | b'`') && first == last && s.len() >= 2 {
        return true;
    }
    // Number
    if s.bytes().all(|b| b.is_ascii_digit() || b == b'.') && !s.is_empty() {
        return true;
    }
    // Keywords
    matches!(s, "true" | "false" | "null" | "undefined")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if let Some(col) = find_default_reassignment(trimmed) {
                let offset = line.find(trimmed).unwrap_or(0);
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: offset + col + 1,
                    rule_id: "prefer-default-parameters".into(),
                    message: "Prefer default parameters over reassignment.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_logical_or_reassignment() {
        let d = run("function f(x) {\n  x = x || 'default';\n}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-default-parameters");
    }

    #[test]
    fn flags_nullish_coalescing_reassignment() {
        let d = run("function f(x) {\n  x = x ?? 42;\n}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_double_quote_string() {
        let d = run("function f(val) {\n  val = val || \"fallback\";\n}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_default_parameter() {
        assert!(run("function f(x = 'default') {}").is_empty());
    }

    #[test]
    fn allows_different_identifiers() {
        assert!(run("function f(x) {\n  x = y || 'default';\n}").is_empty());
    }

    #[test]
    fn allows_non_literal_rhs() {
        assert!(run("function f(x) {\n  x = x || getValue();\n}").is_empty());
    }
}
