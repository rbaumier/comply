use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects lookaround assertions that can be replaced with predefined assertions.
/// Example: `(?=\w)(?<=\W)` or `(?!\w)` at a word boundary position can use `\b`.
/// Example: `(?<=^)` can use `^`, `(?=$)` can use `$`.
fn find_replaceable_assertions(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();

    // Patterns that can be replaced with \b or \B
    let replaceable: &[&str] = &[
        "(?=\\w)(?<=\\W)",
        "(?=\\W)(?<=\\w)",
        "(?<=\\w)(?=\\W)",
        "(?<=\\W)(?=\\w)",
    ];

    // Patterns replaceable with ^ or $
    let anchor_replaceable: &[&str] = &[
        "(?<=^)",
        "(?=$)",
    ];

    let mut i = 0;
    while i < len {
        if !line.is_char_boundary(i) {
            i += 1;
            continue;
        }
        for pat in replaceable.iter().chain(anchor_replaceable.iter()) {
            if line.get(i..i + pat.len()) == Some(*pat) {
                hits.push(i);
                break;
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
            for col in find_replaceable_assertions(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-prefer-predefined-assertion".into(),
                    message: "This lookaround can be replaced with a predefined assertion like `\\b`, `^`, or `$`.".into(),
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
    fn flags_word_boundary_lookaround() {
        assert_eq!(run(r"const re = /(?=\w)(?<=\W)/;").len(), 1);
    }

    #[test]
    fn flags_start_anchor_lookaround() {
        assert_eq!(run(r"const re = /(?<=^)foo/;").len(), 1);
    }

    #[test]
    fn allows_normal_lookaround() {
        assert!(run(r"const re = /(?=foo)/;").is_empty());
    }
}
