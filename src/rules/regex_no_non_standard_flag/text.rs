use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const STANDARD_FLAGS: &[u8] = b"dgimsuvy";

/// Detects non-standard regex flags.
fn find_non_standard_flags(line: &str) -> Vec<usize> {
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
                    // Extract flags
                    let flag_start = j + 1;
                    let mut flag_end = flag_start;
                    while flag_end < len && bytes[flag_end].is_ascii_alphabetic() {
                        flag_end += 1;
                    }
                    if flag_end > flag_start {
                        let flags = &bytes[flag_start..flag_end];
                        for &f in flags {
                            if !STANDARD_FLAGS.contains(&f) {
                                hits.push(i);
                                break;
                            }
                        }
                    }
                    i = flag_end;
                    break;
                }
                if bytes[j] == b'\n' {
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
            for col in find_non_standard_flags(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-non-standard-flag".into(),
                    message: "Non-standard regex flag detected \u{2014} standard flags are: d, g, i, m, s, u, v, y.".into(),
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
    fn flags_non_standard_flag() {
        assert_eq!(run(r#"const re = /foo/x;"#).len(), 1);
    }

    #[test]
    fn allows_standard_flags() {
        assert!(run(r#"const re = /foo/gim;"#).is_empty());
    }

    #[test]
    fn flags_unknown_flag_l() {
        assert_eq!(run(r#"const re = /bar/l;"#).len(), 1);
    }
}
