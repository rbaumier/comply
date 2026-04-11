use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect relational operators (`<`, `>`, `<=`, `>=`) used with string literals.
/// Matches: `"abc" < "def"`, `x > "foo"`, `"bar" >= someVar`, etc.
fn has_string_relational_comparison(line: &str) -> bool {
    // Look for patterns: <string> <op> or <op> <string>
    // where <op> is one of: <=, >=, <, > (but not << or >> or => or <!--)
    let ops = ["<=", ">="];
    let single_ops = ['<', '>'];

    // Check for two-char operators first
    for op in &ops {
        if let Some(pos) = line.find(op) {
            let left = line[..pos].trim();
            let right = line[pos + op.len()..].trim();
            if is_string_literal_end(left) || is_string_literal_start(right) {
                return true;
            }
        }
    }

    // Check single-char operators (skip if part of <=, >=, =>, <<, >>, or -->)
    for &op in &single_ops {
        let bytes = line.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b != op as u8 {
                continue;
            }
            // Skip multi-char operators
            if op == '<' && i + 1 < bytes.len() && (bytes[i + 1] == b'=' || bytes[i + 1] == b'<') {
                continue;
            }
            if op == '>' && i + 1 < bytes.len() && (bytes[i + 1] == b'=' || bytes[i + 1] == b'>') {
                continue;
            }
            if op == '>' && i > 0 && (bytes[i - 1] == b'=' || bytes[i - 1] == b'-') {
                continue;
            }
            if op == '<' && i > 0 && bytes[i - 1] == b'<' {
                continue;
            }

            let left = line[..i].trim();
            let right = line[i + 1..].trim();
            if is_string_literal_end(left) || is_string_literal_start(right) {
                return true;
            }
        }
    }

    false
}

fn is_string_literal_end(s: &str) -> bool {
    (s.ends_with('"') && !s.ends_with("\\\"")) || (s.ends_with('\'') && !s.ends_with("\\'"))
}

fn is_string_literal_start(s: &str) -> bool {
    s.starts_with('"') || s.starts_with('\'')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_string_relational_comparison(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "strings-comparison".into(),
                    message: "Relational comparison with string literal uses lexicographic order \u{2014} this is rarely the intent.".into(),
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
    fn flags_string_less_than() {
        assert_eq!(run(r#"if ("abc" < "def") {}"#).len(), 1);
    }

    #[test]
    fn flags_var_greater_than_string() {
        assert_eq!(run(r#"if (name > "xyz") {}"#).len(), 1);
    }

    #[test]
    fn flags_string_gte() {
        assert_eq!(run(r#"return str >= "aaa";"#).len(), 1);
    }

    #[test]
    fn allows_equality_comparison() {
        assert!(run(r#"if (x === "hello") {}"#).is_empty());
    }

    #[test]
    fn allows_number_comparison() {
        assert!(run(r#"if (x > 5) {}"#).is_empty());
    }
}
