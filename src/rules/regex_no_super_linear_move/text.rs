use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects quantifiers that can cause quadratic runtime. A quantifier followed
/// by the same character/class it matches can cause super-linear backtracking.
/// Example: `/a+a/` — the `a+` followed by `a` forces re-scanning.
fn find_super_linear(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 1 < len {
        // Pattern: X+X or X*X where X is the same literal char
        if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' {
            let ch = bytes[i];
            if i + 1 < len && (bytes[i + 1] == b'+' || bytes[i + 1] == b'*') {
                let after_quant = i + 2;
                // Skip `?` for lazy quantifier
                let check_pos = if after_quant < len && bytes[after_quant] == b'?' {
                    after_quant + 1
                } else {
                    after_quant
                };
                if check_pos < len && bytes[check_pos] == ch {
                    hits.push(i);
                }
            }
        }
        i += 1;
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_super_linear(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-super-linear-move".into(),
                    message: "Quantifier followed by the same element can cause quadratic runtime.".into(),
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
    fn flags_plus_followed_by_same() {
        assert_eq!(run(r#"const re = /a+a/;"#).len(), 1);
    }

    #[test]
    fn flags_star_followed_by_same() {
        assert_eq!(run(r#"const re = /a*a/;"#).len(), 1);
    }

    #[test]
    fn allows_different_char_after_quantifier() {
        assert!(run(r#"const re = /a+b/;"#).is_empty());
    }
}
