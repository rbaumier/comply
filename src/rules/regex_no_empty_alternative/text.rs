use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::rust_helpers::extract_rust_regex_patterns;

#[derive(Debug)]
pub struct Check;

/// Extract the pattern from a regex literal `/pattern/flags` or return None.
fn extract_regex_pattern(line: &str) -> Option<&str> {
    // Look for regex literal: /pattern/
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' {
            // Possible start of regex — skip if preceded by alphanumeric or closing bracket
            if i > 0 {
                let prev = bytes[i - 1];
                if prev.is_ascii_alphanumeric() || prev == b')' || prev == b']' {
                    i += 1;
                    continue;
                }
            }
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2; // skip escaped char
                    continue;
                }
                if bytes[j] == b'/' {
                    return Some(&line[start..j]);
                }
                if bytes[j] == b'\n' {
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }
    None
}

fn pattern_has_empty_alternative(pattern: &str) -> bool {
    pattern.starts_with('|') || pattern.ends_with('|') || pattern.contains("||")
}

/// Check for empty alternatives: `|` at start, end, or consecutive `||`.
fn has_empty_alternative(line: &str) -> bool {
    if let Some(pattern) = extract_regex_pattern(line)
        && pattern_has_empty_alternative(pattern) {
            return true;
        }
    // Also check RegExp constructor
    if let Some(pos) = line.find("RegExp(") {
        let rest = &line[pos + 7..];
        if let Some(q) = rest.find(['"', '\'']) {
            let quote = rest.as_bytes()[q];
            let inner = &rest[q + 1..];
            if let Some(end) = inner.find(quote as char) {
                let pattern = &inner[..end];
                if pattern_has_empty_alternative(pattern) {
                    return true;
                }
            }
        }
    }
    // Check Rust Regex::new(...)
    for (_col, pattern) in extract_rust_regex_patterns(line) {
        if pattern_has_empty_alternative(pattern) {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_empty_alternative(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-empty-alternative".into(),
                    message: "Empty alternative in regex — remove leading, trailing, or consecutive `|`.".into(),
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
    fn flags_leading_pipe() {
        assert_eq!(run("const re = /|foo/;").len(), 1);
    }

    #[test]
    fn flags_trailing_pipe() {
        assert_eq!(run("const re = /foo|/;").len(), 1);
    }

    #[test]
    fn flags_consecutive_pipes() {
        assert_eq!(run("const re = /foo||bar/;").len(), 1);
    }

    #[test]
    fn flags_regexp_constructor() {
        assert_eq!(run("const re = new RegExp(\"|foo\");").len(), 1);
    }

    #[test]
    fn allows_valid_alternatives() {
        assert!(run("const re = /foo|bar/;").is_empty());
    }

    fn run_rs(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), source))
    }

    #[test]
    fn flags_rust_regex_leading_pipe() {
        assert_eq!(run_rs(r#"let re = Regex::new(r"|foo");"#).len(), 1);
    }

    #[test]
    fn allows_rust_regex_valid_alternatives() {
        assert!(run_rs(r#"let re = Regex::new(r"foo|bar");"#).is_empty());
    }
}
