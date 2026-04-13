use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects patterns like `(?=\d)\w` or `(?!\d)\w` that can be expressed
/// using `v`-flag set operations: `[\d&&\w]` or `[\w--\d]`.
fn find_set_operation_candidates(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();

    let patterns: &[&str] = &[
        "(?=\\d)\\w",
        "(?=\\w)\\d",
        "(?!\\d)\\w",
        "(?!\\w)\\d",
        "(?=\\s)\\w",
        "(?=\\w)\\s",
        "(?!\\s)\\w",
        "(?!\\w)\\s",
    ];

    for pat in patterns {
        let mut search_from = 0;
        while let Some(pos) = line[search_from..].find(pat) {
            hits.push(search_from + pos);
            search_from += pos + pat.len();
        }
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_set_operation_candidates(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-prefer-set-operation".into(),
                    message: "This lookaround + character pattern can be expressed using a v-flag set operation.".into(),
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
    fn flags_lookahead_with_char_class() {
        assert_eq!(run(r"const re = /(?=\d)\w/;").len(), 1);
    }

    #[test]
    fn flags_negative_lookahead_char_class() {
        assert_eq!(run(r"const re = /(?!\d)\w/;").len(), 1);
    }

    #[test]
    fn allows_unrelated_lookahead() {
        assert!(run(r"const re = /(?=foo)bar/;").is_empty());
    }
}
