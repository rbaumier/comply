use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects useless nested lookaround assertions that can be inlined.
/// Example: `(?=(?=a))` — the inner lookahead is unnecessary.
fn find_extra_lookarounds(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        // Look for (?= or (?! or (?<= or (?<!
        if bytes[i] == b'(' && bytes[i + 1] == b'?' {
            let is_lookaround = matches!(bytes[i + 2], b'=' | b'!')
                || (bytes[i + 2] == b'<' && i + 3 < len && matches!(bytes[i + 3], b'=' | b'!'));

            if is_lookaround {
                let content_start = if bytes[i + 2] == b'<' { i + 4 } else { i + 3 };
                // Check if the only content is another lookaround of the same kind
                let trimmed = &line[content_start..];
                if trimmed.starts_with("(?=") || trimmed.starts_with("(?!") || trimmed.starts_with("(?<=") || trimmed.starts_with("(?<!") {
                    // Verify the inner lookaround closes at the right place
                    if let Some(inner_close) = find_matching_paren(bytes, content_start) {
                        // Check that the outer group closes right after
                        if inner_close + 1 < len && bytes[inner_close + 1] == b')' {
                            hits.push(i);
                        }
                    }
                }
            }
        }
        i += 1;
    }
    hits
}

fn find_matching_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_extra_lookarounds(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-extra-lookaround-assertions".into(),
                    message: "Useless nested lookaround assertion \u{2014} it can be inlined.".into(),
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
    fn flags_nested_lookahead() {
        assert_eq!(run(r#"const re = /(?=(?=a))/;"#).len(), 1);
    }

    #[test]
    fn allows_single_lookahead() {
        assert!(run(r#"const re = /(?=a)/;"#).is_empty());
    }

    #[test]
    fn flags_nested_negative_lookahead() {
        assert_eq!(run(r#"const re = /(?!(?!a))/;"#).len(), 1);
    }
}
