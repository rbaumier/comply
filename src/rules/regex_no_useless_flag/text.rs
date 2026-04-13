use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects regex flags that have no effect on the pattern.
/// - `i` flag when pattern has no letters
/// - `m` flag when pattern has no `^` or `$`
/// - `s` flag when pattern has no `.`
fn find_useless_flags(line: &str) -> Vec<usize> {
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
                    let flag_start = j + 1;
                    let mut flag_end = flag_start;
                    while flag_end < len && bytes[flag_end].is_ascii_alphabetic() {
                        flag_end += 1;
                    }
                    let flags = &line[flag_start..flag_end];

                    if has_useless_flag(pattern, flags) {
                        hits.push(i);
                    }
                    i = flag_end;
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }
    hits
}

fn has_useless_flag(pattern: &str, flags: &str) -> bool {
    let pbytes = pattern.as_bytes();

    // `i` flag with no unescaped letters
    if flags.contains('i') {
        let mut has_letter = false;
        let mut k = 0;
        while k < pbytes.len() {
            if pbytes[k] == b'\\' {
                k += 2; // skip escape sequences like \d, \w, \s
                continue;
            }
            if pbytes[k] == b'[' {
                // skip character class content — not literal
                k += 1;
                while k < pbytes.len() && pbytes[k] != b']' {
                    if pbytes[k] == b'\\' { k += 1; }
                    k += 1;
                }
            }
            if k < pbytes.len() && pbytes[k].is_ascii_alphabetic() {
                has_letter = true;
                break;
            }
            k += 1;
        }
        if !has_letter {
            return true;
        }
    }

    // `m` flag with no ^ or $
    if flags.contains('m') {
        let has_anchor = pbytes.contains(&b'^') || pbytes.contains(&b'$');
        if !has_anchor {
            return true;
        }
    }

    // `s` flag with no .
    if flags.contains('s') {
        // Check for unescaped dot
        let mut k = 0;
        let mut has_dot = false;
        while k < pbytes.len() {
            if pbytes[k] == b'\\' {
                k += 2;
                continue;
            }
            if pbytes[k] == b'.' {
                has_dot = true;
                break;
            }
            k += 1;
        }
        if !has_dot {
            return true;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_useless_flags(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-useless-flag".into(),
                    message: "Regex flag has no effect on this pattern \u{2014} remove it.".into(),
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
    fn flags_useless_i_flag() {
        assert_eq!(run(r#"const re = /\d+/i;"#).len(), 1);
    }

    #[test]
    fn allows_useful_i_flag() {
        assert!(run(r#"const re = /foo/i;"#).is_empty());
    }

    #[test]
    fn flags_useless_m_flag() {
        assert_eq!(run(r#"const re = /foo/m;"#).len(), 1);
    }

    #[test]
    fn flags_useless_s_flag() {
        assert_eq!(run(r#"const re = /foo/s;"#).len(), 1);
    }
}
