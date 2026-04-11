use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const OPERATORS: &[&str] = &["===", "!==", "&&", "||", "-", "/"];

/// Check whether a line contains `expr OP expr` where both sides are identical.
fn find_identical_binary(line: &str) -> Option<(&str, &str)> {
    for &op in OPERATORS {
        let mut start = 0;
        while let Some(pos) = line[start..].find(op) {
            let abs = start + pos;
            let lhs_end = abs;
            let rhs_start = abs + op.len();

            // Extract token-like text on each side.
            let lhs = line[..lhs_end].trim_end();
            let rhs = line[rhs_start..].trim_start();

            // Take the last "token" from LHS and first "token" from RHS.
            let lhs_token = last_token(lhs);
            let rhs_token = first_token(rhs);

            if !lhs_token.is_empty()
                && !rhs_token.is_empty()
                && lhs_token == rhs_token
                // Avoid false positives on single-char tokens for `-` and `/`.
                && (lhs_token.len() > 1 || (op != "-" && op != "/"))
            {
                return Some((lhs_token, op));
            }

            start = rhs_start;
        }
    }
    None
}

fn last_token(s: &str) -> &str {
    let s = s.trim_end();
    // Walk backwards to find the start of an identifier/number token.
    let end = s.len();
    let start = s
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.' && c != '$')
        .map(|i| i + 1)
        .unwrap_or(0);
    &s[start..end]
}

fn first_token(s: &str) -> &str {
    let s = s.trim_start();
    let end = s
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.' && c != '$')
        .unwrap_or(s.len());
    &s[..end]
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Skip comments.
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            if let Some((expr, op)) = find_identical_binary(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-identical-expressions".into(),
                    message: format!("Identical expression `{}` on both sides of `{}`.", expr, op),
                    severity: Severity::Error,
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
    fn flags_identical_strict_eq() {
        let d = run("if (a === a) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("==="));
    }

    #[test]
    fn flags_identical_and() {
        let d = run("const ok = valid && valid;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("&&"));
    }

    #[test]
    fn flags_identical_subtraction() {
        let d = run("const zero = count - count;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_different_sides() {
        assert!(run("if (a === b) {}").is_empty());
    }

    #[test]
    fn allows_comments() {
        assert!(run("// a === a is fine in comments").is_empty());
    }
}
