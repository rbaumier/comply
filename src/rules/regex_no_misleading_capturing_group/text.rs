use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects capturing groups whose alternatives start or end with different
/// character types, making the capture misleading, e.g. `(a+|b+)c` where the
/// capture contents vary confusingly.
fn find_misleading_captures(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Match opening `(` that is NOT `(?` (non-capturing / lookaround)
        if bytes[i] == b'(' && i + 1 < len && bytes[i + 1] != b'?' {
            // Find the matching close paren
            let start = i;
            let mut depth = 1;
            let mut j = i + 1;
            let mut has_alternation = false;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'|' if depth == 1 => has_alternation = true,
                    _ => {}
                }
                j += 1;
            }
            // A capturing group with alternation followed by a quantifier is misleading
            if depth == 0 && has_alternation && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'+' || next == b'*' || next == b'?' || next == b'{' {
                    hits.push(start);
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
            for col in find_misleading_captures(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-misleading-capturing-group".into(),
                    message: "Capturing group with alternation and quantifier is misleading \u{2014} the capture may match different things.".into(),
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
    fn flags_alternation_with_quantifier() {
        assert_eq!(run(r#"const re = /(a|b)+/;"#).len(), 1);
    }

    #[test]
    fn allows_capturing_without_quantifier() {
        assert!(run(r#"const re = /(a|b)/;"#).is_empty());
    }

    #[test]
    fn flags_alternation_with_star() {
        assert_eq!(run(r#"const re = /(foo|bar)*/;"#).len(), 1);
    }
}
