use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

fn likely_in_string_or_comment(line: &str, match_start: usize) -> bool {
    let prefix = &line[..match_start];
    if prefix.contains("//") {
        return true;
    }
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut prev_backslash = false;
    for ch in prefix.chars() {
        if prev_backslash {
            prev_backslash = false;
            continue;
        }
        if ch == '\\' {
            prev_backslash = true;
            continue;
        }
        match ch {
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            _ => {}
        }
    }
    in_single || in_double || in_backtick
}

/// Scan for numeric literals like `1.0`, `1.00`, `1.` (dangling dot).
/// Returns (start_byte, is_dangling) for each match found.
fn find_zero_fraction_literals(line: &str) -> Vec<(usize, bool)> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut results = Vec::new();
    let mut i = 0;

    while i < len {
        // Find a digit that starts a number
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }

        // Check that it's not preceded by a word char (part of identifier)
        if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
            // Skip the rest of this word
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            continue;
        }

        let start = i;

        // Consume digits and underscores (integer part)
        while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
            i += 1;
        }

        // Must be followed by a dot
        if i >= len || bytes[i] != b'.' {
            continue;
        }

        // Check for range operator `..`
        if i + 1 < len && bytes[i + 1] == b'.' {
            i += 1;
            continue;
        }

        let dot_pos = i;
        i += 1; // skip the dot

        // Consume trailing zeros and underscores
        while i < len && (bytes[i] == b'0' || bytes[i] == b'_') {
            i += 1;
        }

        // If followed by a non-zero digit, it's a real fraction -- skip
        if i < len && bytes[i].is_ascii_digit() {
            // Skip rest of number
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                i += 1;
            }
            continue;
        }

        // If followed by a word char, skip (e.g. method call)
        if i < len && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
            continue;
        }

        let is_dangling = i == dot_pos + 1; // nothing after the dot
        results.push((start, is_dangling));
    }

    results
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            for (col, is_dangling) in find_zero_fraction_literals(line) {
                if likely_in_string_or_comment(line, col) {
                    continue;
                }

                let msg = if is_dangling {
                    "Don't use a dangling dot in the number."
                } else {
                    "Don't use a zero fraction in the number."
                };

                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "no-zero-fractions".into(),
                    message: msg.into(),
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
    fn flags_zero_fraction() {
        let d = run("const x = 1.0;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("zero fraction"));
    }

    #[test]
    fn flags_multiple_zero_fraction() {
        let d = run("const x = 1.00;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_dangling_dot() {
        let d = run("const x = 1. ;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("dangling dot"));
    }

    #[test]
    fn allows_real_fraction() {
        assert!(run("const x = 1.5;").is_empty());
    }

    #[test]
    fn allows_integer() {
        assert!(run("const x = 1;").is_empty());
    }

    #[test]
    fn allows_non_zero_fraction() {
        assert!(run("const x = 3.14;").is_empty());
    }

    #[test]
    fn ignores_strings() {
        assert!(run(r#"const x = "1.0";"#).is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// const x = 1.0;").is_empty());
    }

    #[test]
    fn does_not_match_range_operator() {
        assert!(run("for (let i of range(1..10)) {}").is_empty());
    }
}
