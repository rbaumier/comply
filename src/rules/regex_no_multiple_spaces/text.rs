use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::rust_helpers::extract_rust_regex_patterns;

#[derive(Debug)]
pub struct Check;

/// Extract the regex pattern from a literal `/pattern/` and check for 2+ consecutive spaces.
fn has_multiple_spaces_in_regex(line: &str) -> bool {
    // Check regex literals
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' {
            // Skip if preceded by alphanumeric (likely division)
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
                    j += 2;
                    continue;
                }
                if bytes[j] == b'/' {
                    // Found end of regex, check pattern for consecutive spaces
                    let pattern = &line[start..j];
                    if pattern.contains("  ") {
                        return true;
                    }
                    break;
                }
                if bytes[j] == b'\n' {
                    break;
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    // Check RegExp constructor
    if let Some(pos) = line.find("RegExp(") {
        let rest = &line[pos + 7..];
        if let Some(q) = rest.find(['"', '\'']) {
            let quote = rest.as_bytes()[q];
            let inner = &rest[q + 1..];
            if let Some(end) = inner.find(quote as char) {
                let pattern = &inner[..end];
                if pattern.contains("  ") {
                    return true;
                }
            }
        }
    }
    // Check Rust Regex::new(...)
    for (_col, pattern) in extract_rust_regex_patterns(line) {
        if pattern.contains("  ") {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_multiple_spaces_in_regex(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-multiple-spaces".into(),
                    message: "Multiple consecutive spaces in regex — use a quantifier like ` {2}` instead.".into(),
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
    fn flags_double_space_in_literal() {
        assert_eq!(run("const re = /foo  bar/;").len(), 1);
    }

    #[test]
    fn flags_triple_space_in_regexp() {
        assert_eq!(run("const re = new RegExp(\"foo   bar\");").len(), 1);
    }

    #[test]
    fn allows_single_space() {
        assert!(run("const re = /foo bar/;").is_empty());
    }

    #[test]
    fn allows_quantifier() {
        assert!(run("const re = / {2}/;").is_empty());
    }

    fn run_rs(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), source))
    }

    #[test]
    fn flags_rust_regex_double_space() {
        assert_eq!(run_rs(r#"let re = Regex::new(r"foo  bar");"#).len(), 1);
    }

    #[test]
    fn allows_rust_regex_single_space() {
        assert!(run_rs(r#"let re = Regex::new(r"foo bar");"#).is_empty());
    }
}
