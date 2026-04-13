use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect reluctant quantifiers (`*?`, `+?`, `??`) followed by end-of-pattern,
/// `$`, or `)`.
fn has_useless_reluctant(line: &str) -> bool {
    // Match patterns like *?$, +?$, ??$, *?), +?), ??), *?/, +?/, ??/
    let bytes = line.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        let q = bytes[i];
        if (q == b'*' || q == b'+' || q == b'?') && bytes[i + 1] == b'?' {
            let next = bytes[i + 2];
            if next == b'$' || next == b')' || next == b'/' {
                return true;
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_useless_reluctant(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-empty-after-reluctant".into(),
                    message: "Reluctant quantifier before end-of-pattern is useless — it always matches the minimum.".into(),
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
    fn flags_reluctant_star_before_dollar() {
        assert_eq!(run("const re = /a*?$/;").len(), 1);
    }

    #[test]
    fn flags_reluctant_plus_before_close_paren() {
        assert_eq!(run("const re = /(?:a+?)/;").len(), 1);
    }

    #[test]
    fn flags_reluctant_question_before_end() {
        assert_eq!(run("const re = /x??/;").len(), 1);
    }

    #[test]
    fn allows_reluctant_followed_by_content() {
        assert!(run("const re = /a*?b/;").is_empty());
    }

    #[test]
    fn allows_greedy_before_dollar() {
        assert!(run("const re = /a*$/;").is_empty());
    }
}
