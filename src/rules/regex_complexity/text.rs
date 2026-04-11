use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const THRESHOLD: usize = 20;

/// Extract regex body from a line containing `/pattern/` or `new RegExp("pattern")`.
/// Returns the pattern string if found.
fn extract_regex_pattern(line: &str) -> Option<&str> {
    // Match /pattern/flags
    let trimmed = line.trim();
    if let Some(start) = trimmed.find('/') {
        let rest = &trimmed[start + 1..];
        // Find closing `/` (not escaped)
        let mut i = 0;
        let bytes = rest.as_bytes();
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'/' {
                return Some(&rest[..i]);
            }
            i += 1;
        }
    }
    None
}

/// Score regex complexity by counting special constructs.
fn complexity_score(pattern: &str) -> usize {
    let mut score = 0;
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                // Escaped char — check for assertions
                if i + 1 < bytes.len() {
                    match bytes[i + 1] {
                        b'b' | b'B' => score += 1,
                        _ => {}
                    }
                }
                i += 2;
                continue;
            }
            b'*' | b'+' | b'?' => score += 1,
            b'{' => score += 1,
            b'|' => score += 1,
            b'(' => score += 1,
            b'[' => score += 1,
            b'^' | b'$' => score += 1,
            _ => {}
        }
        i += 1;
    }
    score
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(pattern) = extract_regex_pattern(line) {
                let score = complexity_score(pattern);
                if score > THRESHOLD {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "regex-complexity".into(),
                        message: format!(
                            "Regex complexity score is {score} (threshold: {THRESHOLD}) — consider breaking it into smaller patterns."
                        ),
                        severity: Severity::Warning,
                    });
                }
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
    fn flags_complex_regex() {
        // Score: lots of groups, quantifiers, alternations, classes
        let complex =
            r#"const re = /^(a+|b*|c?)(d{2,3})(e|f|g|h)(i+|j*)(k?|l{1})(m|n|o)(p+|q*)(r?)/;"#;
        assert_eq!(run(complex).len(), 1);
    }

    #[test]
    fn allows_simple_regex() {
        assert!(run(r#"const re = /^hello$/;"#).is_empty());
    }

    #[test]
    fn allows_moderate_regex() {
        // Score under threshold
        assert!(run(r#"const re = /\d{3}-\d{4}/;"#).is_empty());
    }
}
