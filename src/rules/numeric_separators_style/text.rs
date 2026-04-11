use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// Insert underscores every `group` digits from right to left.
fn add_separators(digits: &str, group: usize) -> String {
    let clean: String = digits.chars().filter(|&c| c != '_').collect();
    if clean.len() < group + 1 {
        return clean;
    }
    let mut result = Vec::new();
    for (i, ch) in clean.chars().rev().enumerate() {
        if i > 0 && i % group == 0 {
            result.push('_');
        }
        result.push(ch);
    }
    result.reverse();
    result.into_iter().collect()
}

/// Format a prefixed literal (0x, 0b, 0o) with proper separators.
fn format_prefixed(prefix: &str, digits: &str, suffix: &str) -> String {
    let group = match prefix.to_lowercase().as_str() {
        "0x" => 2,
        "0b" | "0o" => 4,
        _ => return format!("{}{}{}", prefix, digits, suffix),
    };
    let formatted = add_separators(digits, group);
    format!("{}{}{}", prefix, formatted, suffix)
}

/// Format a decimal number with proper separators (groups of 3, min 5 digits).
fn format_decimal(digits: &str, suffix: &str) -> String {
    let clean: String = digits.chars().filter(|&c| c != '_').collect();
    if clean.len() < 5 {
        return format!("{}{}", clean, suffix);
    }
    let formatted = add_separators(digits, 3);
    format!("{}{}", formatted, suffix)
}

/// Scan a line for numeric literals that need separators.
/// Returns (start_byte, raw_text, formatted_text).
fn find_unseparated(line: &str) -> Vec<(usize, String, String)> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut results = Vec::new();
    let mut i = 0;

    while i < len {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }

        // Must not be preceded by a word char
        if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            continue;
        }

        let start = i;

        // Check for prefixed literal
        if bytes[i] == b'0' && i + 1 < len {
            let next = bytes[i + 1];
            if next == b'x'
                || next == b'X'
                || next == b'b'
                || next == b'B'
                || next == b'o'
                || next == b'O'
            {
                let prefix = &line[i..i + 2];
                i += 2;
                let digits_start = i;
                while i < len && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'_') {
                    i += 1;
                }
                let digits = &line[digits_start..i];
                let suffix = if i < len && bytes[i] == b'n' {
                    i += 1;
                    "n"
                } else {
                    ""
                };
                let raw = &line[start..i];
                let formatted = format_prefixed(prefix, digits, suffix);
                if raw != formatted {
                    results.push((start, raw.to_string(), formatted));
                }
                continue;
            }
        }

        // Decimal literal
        while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
            i += 1;
        }

        // Skip floats (with dots) and exponents — they're more complex
        if i < len && (bytes[i] == b'.' || bytes[i] == b'e' || bytes[i] == b'E') {
            // Skip the rest of the number
            i += 1;
            if i < len && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.') {
                i += 1;
            }
            continue;
        }

        let suffix = if i < len && bytes[i] == b'n' {
            i += 1;
            "n"
        } else {
            ""
        };

        let digits = &line[start..i - suffix.len()];
        let raw = &line[start..i];
        let formatted = format_decimal(digits, suffix);
        if raw != formatted {
            results.push((start, raw.to_string(), formatted));
        }
    }

    results
}

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

            for (col, raw, formatted) in find_unseparated(line) {
                if likely_in_string_or_comment(line, col) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "numeric-separators-style".into(),
                    message: format!(
                        "Invalid group length in numeric value: `{}` should be `{}`.",
                        raw, formatted
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
    fn flags_large_decimal_without_separators() {
        let d = run("const x = 1000000;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("1_000_000"));
    }

    #[test]
    fn flags_five_digit_number() {
        let d = run("const x = 10000;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("10_000"));
    }

    #[test]
    fn allows_four_digit_number() {
        assert!(run("const x = 1000;").is_empty());
    }

    #[test]
    fn allows_already_separated() {
        assert!(run("const x = 1_000_000;").is_empty());
    }

    #[test]
    fn flags_hex_without_separators() {
        let d = run("const x = 0xFF00FF;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF_00_FF"));
    }

    #[test]
    fn allows_short_hex() {
        assert!(run("const x = 0xFF;").is_empty());
    }

    #[test]
    fn ignores_strings() {
        assert!(run(r#"const x = "1000000";"#).is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// const x = 1000000;").is_empty());
    }
}
