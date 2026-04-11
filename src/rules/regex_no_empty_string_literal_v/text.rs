use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects empty string literals in `v`-flag regex character classes.
/// Example: `/[\q{}]/v` — empty `\q{}` is unexpected.
fn find_empty_string_literal_v(line: &str) -> Vec<usize> {
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
                    // Check flags for 'v'
                    let flag_start = j + 1;
                    let mut flag_end = flag_start;
                    while flag_end < len && bytes[flag_end].is_ascii_alphabetic() {
                        flag_end += 1;
                    }
                    let flags = &line[flag_start..flag_end];
                    if flags.contains('v') {
                        let pattern = &line[start..j];
                        if pattern.contains("\\q{}") {
                            hits.push(i);
                        }
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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_empty_string_literal_v(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-empty-string-literal-v".into(),
                    message: "Empty string literal in v-flag character class is unexpected.".into(),
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
    fn flags_empty_q_in_v_flag() {
        assert_eq!(run(r#"const re = /[\q{}]/v;"#).len(), 1);
    }

    #[test]
    fn allows_non_v_flag() {
        assert!(run(r#"const re = /[\q{}]/g;"#).is_empty());
    }

    #[test]
    fn allows_non_empty_q() {
        assert!(run(r#"const re = /[\q{ab}]/v;"#).is_empty());
    }
}
