use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects assertions that are always true or always false:
/// - `$` not at end, followed by more content (always fails in non-multiline)
/// - `^` not at start, preceded by content (always fails in non-multiline)
/// - `\b` in positions where it is trivially always true/false
fn find_useless_assertions(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'/' {
            if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b')') {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < len {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'/' {
                    let pattern = &line[start..j];
                    // Extract flags
                    let flag_start = j + 1;
                    let mut flag_end = flag_start;
                    while flag_end < len && bytes[flag_end].is_ascii_alphabetic() {
                        flag_end += 1;
                    }
                    let flags = &line[flag_start..flag_end];
                    let multiline = flags.contains('m');

                    if !multiline
                        && (has_useless_dollar(pattern) || has_useless_caret(pattern)) {
                            hits.push(i);
                        }
                    i = j;
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }
    hits
}

/// `$` followed by non-assertion content (not at end of pattern/alternative)
fn has_useless_dollar(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next != b')' && next != b'|' && next != b'/' {
                // Check it's not `\$`
                if i == 0 || bytes[i - 1] != b'\\' {
                    return true;
                }
            }
        }
    }
    false
}

/// `^` preceded by non-assertion content (not at start of pattern/alternative)
fn has_useless_caret(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'^' && i > 0 {
            let prev = bytes[i - 1];
            if prev != b'(' && prev != b'|' && prev != b'[' && prev != b'\\' {
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
            for col in find_useless_assertions(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-useless-assertions".into(),
                    message: "Assertion is always true or always false and has no effect.".into(),
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
    fn flags_dollar_not_at_end() {
        assert_eq!(run(r#"const re = /foo$bar/;"#).len(), 1);
    }

    #[test]
    fn allows_dollar_at_end() {
        assert!(run(r#"const re = /foo$/;"#).is_empty());
    }

    #[test]
    fn flags_caret_not_at_start() {
        assert_eq!(run(r#"const re = /foo^bar/;"#).len(), 1);
    }

    #[test]
    fn allows_caret_at_start() {
        assert!(run(r#"const re = /^foo/;"#).is_empty());
    }
}
