use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// The canonical form: lowercase prefix/exponent, uppercase hex digits.
fn canonical(raw: &str) -> Option<String> {
    let (body, suffix) = if let Some(stripped) = raw.strip_suffix('n') {
        (stripped, "n")
    } else {
        (raw, "")
    };

    if body.len() < 2 {
        return None;
    }

    let prefix_lower = body[..2].to_lowercase();
    let fixed = match prefix_lower.as_str() {
        "0x" => {
            // Lowercase prefix, uppercase hex digits
            let digits = &body[2..];
            format!("0x{}{}", digits.to_uppercase(), suffix)
        }
        "0b" | "0o" => {
            // Lowercase prefix, digits stay as-is
            format!("{}{}{}", prefix_lower, &body[2..], suffix)
        }
        _ => {
            // Check for exponent: contains 'E' (should be 'e')
            if !body.contains('E') && !body.contains('e') {
                return None;
            }
            let lowered = body.to_lowercase();
            format!("{}{}", lowered, suffix)
        }
    };

    if fixed == raw {
        None
    } else {
        Some(fixed)
    }
}

/// Scan a line for numeric literals with prefixes or exponents.
/// Returns (start_byte, raw_text, fixed_text).
fn find_bad_literals(line: &str) -> Vec<(usize, String, String)> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut results = Vec::new();
    let mut i = 0;

    while i < len {
        // Look for start of a number: digit or 0x/0b/0o prefix
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

        // Check for prefixed literal: 0x, 0X, 0b, 0B, 0o, 0O
        if bytes[i] == b'0' && i + 1 < len {
            let next = bytes[i + 1];
            if next == b'x'
                || next == b'X'
                || next == b'b'
                || next == b'B'
                || next == b'o'
                || next == b'O'
            {
                i += 2; // skip prefix
                        // Consume hex digits, regular digits, and underscores
                while i < len && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'_') {
                    i += 1;
                }
                // Optional bigint suffix
                if i < len && bytes[i] == b'n' {
                    i += 1;
                }
                let raw = &line[start..i];
                if let Some(fixed) = canonical(raw) {
                    results.push((start, raw.to_string(), fixed));
                }
                continue;
            }
        }

        // Regular number — look for exponent
        while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.') {
            i += 1;
        }

        // Check for exponent
        if i < len && (bytes[i] == b'e' || bytes[i] == b'E') {
            let has_exp = true;
            i += 1;
            // Optional sign
            if i < len && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
            // Exponent digits
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                i += 1;
            }
            if has_exp {
                // Optional bigint suffix
                if i < len && bytes[i] == b'n' {
                    i += 1;
                }
                let raw = &line[start..i];
                if let Some(fixed) = canonical(raw) {
                    results.push((start, raw.to_string(), fixed));
                }
            }
        }
        // If no prefix or exponent, nothing to check for this rule
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

            for (col, raw, fixed) in find_bad_literals(line) {
                if likely_in_string_or_comment(line, col) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "number-literal-case".into(),
                    message: format!(
                        "Invalid number literal casing: `{}` should be `{}`.",
                        raw, fixed
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
    fn flags_uppercase_hex_prefix() {
        let d = run("const x = 0XFF;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn flags_lowercase_hex_digits() {
        let d = run("const x = 0xff;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn flags_uppercase_exponent() {
        let d = run("const x = 1E3;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("1e3"));
    }

    #[test]
    fn flags_uppercase_binary_prefix() {
        let d = run("const x = 0B1010;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0b1010"));
    }

    #[test]
    fn flags_uppercase_octal_prefix() {
        let d = run("const x = 0O777;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0o777"));
    }

    #[test]
    fn allows_correct_hex() {
        assert!(run("const x = 0xFF;").is_empty());
    }

    #[test]
    fn allows_correct_exponent() {
        assert!(run("const x = 1e3;").is_empty());
    }

    #[test]
    fn allows_correct_binary() {
        assert!(run("const x = 0b1010;").is_empty());
    }

    #[test]
    fn ignores_strings() {
        assert!(run(r#"const x = "0XFF";"#).is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// const x = 0XFF;").is_empty());
    }

    #[test]
    fn flags_bigint_hex() {
        let d = run("const x = 0XFFn;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFFn"));
    }
}
