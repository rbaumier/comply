use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract regex flags after the closing `/` and check if they're sorted.
fn has_unsorted_flags(line: &str) -> bool {
    let bytes = line.as_bytes();
    let len = bytes.len();
    // Find regex literal closing `/` + flags.
    // Walk backwards from each potential flag sequence.
    let mut i = 0;
    let mut in_regex = false;
    while i < len {
        if !in_regex {
            // Look for the opening `/` of a regex literal.
            // Heuristic: `/` preceded by `=`, `(`, `,`, `|`, `!`, `:`, `;`,
            // `{`, `[`, `?`, or at start of trimmed line.
            if bytes[i] == b'/' {
                let prev_non_ws = bytes[..i]
                    .iter()
                    .rev()
                    .find(|&&b| b != b' ' && b != b'\t');
                let is_regex_start = match prev_non_ws {
                    None => true,
                    Some(&b) => matches!(
                        b,
                        b'=' | b'(' | b',' | b'|' | b'!' | b':' | b';' | b'{' | b'[' | b'?'
                            | b'&' | b'<' | b'>'
                    ),
                };
                if is_regex_start {
                    in_regex = true;
                    i += 1;
                    continue;
                }
            }
            i += 1;
        } else {
            // Inside regex — find the closing `/`, skip escaped chars.
            if bytes[i] == b'\\' {
                i += 2; // skip escaped char
                continue;
            }
            if bytes[i] == b'/' {
                // Found closing `/`. Extract flags.
                let flag_start = i + 1;
                let mut flag_end = flag_start;
                while flag_end < len && bytes[flag_end].is_ascii_alphabetic() {
                    flag_end += 1;
                }
                if flag_end > flag_start {
                    let flags = &bytes[flag_start..flag_end];
                    if flags.len() >= 2 {
                        let mut sorted = flags.to_vec();
                        sorted.sort_unstable();
                        if flags != sorted.as_slice() {
                            return true;
                        }
                    }
                }
                in_regex = false;
                i = flag_end;
                continue;
            }
            i += 1;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_unsorted_flags(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-sort-flags".into(),
                    message: "Regex flags are not sorted alphabetically — reorder them (e.g. `dgimsvy`).".into(),
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
    fn flags_unsorted_gi() {
        assert_eq!(run("const re = /foo/ig;").len(), 1);
    }

    #[test]
    fn flags_unsorted_mig() {
        assert_eq!(run("const re = /bar/mig;").len(), 1);
    }

    #[test]
    fn allows_sorted_flags() {
        assert!(run("const re = /foo/gi;").is_empty());
    }

    #[test]
    fn allows_single_flag() {
        assert!(run("const re = /foo/g;").is_empty());
    }

    #[test]
    fn allows_no_flags() {
        assert!(run("const re = /foo/;").is_empty());
    }
}
