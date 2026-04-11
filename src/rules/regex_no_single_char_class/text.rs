use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Matches `[X]` where X is a single non-special character inside a regex.
/// Special chars inside a class that would change meaning: `^`, `]`, `\`.
fn find_single_char_class(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'[' && bytes[i + 1] != b'^' && bytes[i + 1] != b'\\' && bytes[i + 1] != b']' {
            // Check that the next byte after the single char is `]`
            if i + 2 < len && bytes[i + 2] == b']' {
                hits.push(i);
                i += 3;
                continue;
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
            for col in find_single_char_class(line) {
                let snippet = &line[col..col + 3];
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-single-char-class".into(),
                    message: format!(
                        "Unnecessary single-character class `{}` \u{2014} use the character directly (or escape it).",
                        snippet,
                    ),
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
    fn flags_single_char_class() {
        let diags = run(r#"const re = /[a]bc/;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("[a]"));
    }

    #[test]
    fn flags_dot_in_class() {
        let diags = run(r#"const re = /[.]foo/;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("[.]"));
    }

    #[test]
    fn allows_multi_char_class() {
        assert!(run(r#"const re = /[abc]/;"#).is_empty());
    }

    #[test]
    fn allows_negated_class() {
        assert!(run(r#"const re = /[^a]/;"#).is_empty());
    }

    #[test]
    fn allows_escape_in_class() {
        assert!(run(r#"const re = /[\d]/;"#).is_empty());
    }
}
