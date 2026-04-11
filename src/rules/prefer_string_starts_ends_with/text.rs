use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Characters that make a regex pattern "complex" (not a simple literal string).
const SPECIAL_CHARS: &[char] = &['^', '$', '+', '[', '{', '(', '\\', '.', '?', '*', '|'];

/// Returns true if `s` contains none of the special regex characters.
fn is_simple_string(s: &str) -> bool {
    !s.chars().any(|c| SPECIAL_CHARS.contains(&c))
}

/// Check if `flags` contains `i` or `m`, which make the regex unsuitable
/// for a simple startsWith/endsWith replacement.
fn has_bad_flags(flags: &str) -> bool {
    flags.contains('i') || flags.contains('m')
}

/// Try to parse a regex literal `.test(` pattern from the line.
/// Returns diagnostics for `/^simple/.test(` or `/simple$/.test(` patterns.
fn check_line(line: &str, line_num: usize, path: &std::path::Path) -> Vec<Diagnostic> {
    let mut results = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for `/` that starts a regex literal
        if bytes[i] != b'/' {
            i += 1;
            continue;
        }

        let regex_start = i;
        i += 1;

        // Find the closing `/`, tracking escapes
        let mut pattern = String::new();
        let mut found_close = false;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                pattern.push(bytes[i] as char);
                pattern.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if bytes[i] == b'/' {
                found_close = true;
                i += 1;
                break;
            }
            pattern.push(bytes[i] as char);
            i += 1;
        }

        if !found_close || pattern.is_empty() {
            continue;
        }

        // Collect flags
        let mut flags = String::new();
        while i < bytes.len() && bytes[i].is_ascii_lowercase() {
            flags.push(bytes[i] as char);
            i += 1;
        }

        // Must be followed by `.test(`
        let remaining = &line[i..];
        if !remaining.starts_with(".test(") {
            continue;
        }

        if has_bad_flags(&flags) {
            continue;
        }

        // Check for ^prefix pattern
        if let Some(literal) = pattern.strip_prefix('^')
            && is_simple_string(literal) {
                results.push(Diagnostic {
                    path: path.to_path_buf(),
                    line: line_num,
                    column: regex_start + 1,
                    rule_id: "prefer-string-starts-ends-with".into(),
                    message: "Prefer `String#startsWith()` over a regex with `^`.".into(),
                    severity: Severity::Warning,
                });
                break; // one per line
            }

        // Check for suffix$ pattern
        if pattern.ends_with('$') {
            let literal = &pattern[..pattern.len() - 1];
            if is_simple_string(literal) {
                results.push(Diagnostic {
                    path: path.to_path_buf(),
                    line: line_num,
                    column: regex_start + 1,
                    rule_id: "prefer-string-starts-ends-with".into(),
                    message: "Prefer `String#endsWith()` over a regex with `$`.".into(),
                    severity: Severity::Warning,
                });
                break; // one per line
            }
        }
    }

    results
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            diagnostics.extend(check_line(line, idx + 1, ctx.path));
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }

    #[test]
    fn flags_starts_with_regex() {
        let d = run(r#"/^foo/.test(str)"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("startsWith"));
    }

    #[test]
    fn flags_ends_with_regex() {
        let d = run(r#"/bar$/.test(str)"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("endsWith"));
    }

    #[test]
    fn ignores_case_insensitive() {
        assert!(run(r#"/^foo/i.test(str)"#).is_empty());
    }

    #[test]
    fn ignores_multiline() {
        assert!(run(r#"/^foo/m.test(str)"#).is_empty());
    }

    #[test]
    fn allows_complex_regex() {
        // Contains regex special chars — not a simple string
        assert!(run(r#"/^fo+o/.test(str)"#).is_empty());
    }

    #[test]
    fn flags_hyphenated_pattern() {
        let d = run(r#"/^my-prefix/.test(name)"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_test_call() {
        assert!(run(r#"/^foo/.exec(str)"#).is_empty());
    }
}
