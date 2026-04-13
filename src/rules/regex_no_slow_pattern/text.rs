use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects nested quantifiers like `(X+)+`, `(X*)*`, `(X+)*`, `(X*)+`, `(.*)*` etc.
/// These patterns can cause catastrophic backtracking.
fn has_nested_quantifier(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let start = i;
            // Find matching closing paren
            let mut depth = 1;
            let mut j = i + 1;
            let mut inner_has_quantifier = false;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => { j += 1; } // skip escaped char
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'+' | b'*' => inner_has_quantifier = true,
                    _ => {}
                }
                j += 1;
            }
            // j is at closing paren (or end)
            if depth == 0 && inner_has_quantifier && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'+' || next == b'*' {
                    hits.push(start);
                    i = j + 2;
                    continue;
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
            for col in has_nested_quantifier(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-slow-pattern".into(),
                    message: "Nested quantifier detected \u{2014} this pattern can cause catastrophic backtracking (ReDoS).".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn flags_plus_plus() {
        let diags = run(r#"const re = /(a+)+/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_star_star() {
        let diags = run(r#"const re = /(.*)*$/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_plus_star() {
        let diags = run(r#"const re = /(a+)*/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_single_quantifier() {
        assert!(run(r#"const re = /(a+)/;"#).is_empty());
    }

    #[test]
    fn allows_non_quantified_group() {
        assert!(run(r#"const re = /(abc)/;"#).is_empty());
    }
}
