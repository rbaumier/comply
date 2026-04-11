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

/// Check if a string contains only characters valid in a numeric BigInt argument.
fn is_numeric_arg(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let s = s.trim();
    // Allow optional leading + or -
    let s = s
        .strip_prefix('+')
        .or_else(|| s.strip_prefix('-'))
        .unwrap_or(s);
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    // Check for prefix
    if s.len() >= 2 {
        let prefix = &s[..2].to_lowercase();
        if prefix == "0x" || prefix == "0b" || prefix == "0o" {
            return s[2..].chars().all(|c| c.is_ascii_hexdigit() || c == '_');
        }
    }
    // Plain decimal
    s.chars().all(|c| c.is_ascii_digit() || c == '_')
}

/// Find `BigInt(literal)` calls and return (start, full_match, replacement).
fn find_bigint_calls(line: &str) -> Vec<(usize, String, String)> {
    let mut results = Vec::new();
    let mut search_from = 0;

    while let Some(pos) = line[search_from..].find("BigInt") {
        let abs_pos = search_from + pos;

        // Check it's a word boundary (not part of a larger identifier)
        if abs_pos > 0 {
            let prev = line.as_bytes()[abs_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                search_from = abs_pos + 6;
                continue;
            }
        }

        let after_bigint = abs_pos + 6;

        // Skip optional whitespace, then expect '('
        let mut i = after_bigint;
        let bytes = line.as_bytes();
        let len = bytes.len();

        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if i >= len || bytes[i] != b'(' {
            search_from = after_bigint;
            continue;
        }

        let _paren_open = i;
        i += 1; // skip '('

        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if i >= len {
            search_from = after_bigint;
            continue;
        }

        // Determine argument type
        let arg_start = i;
        let (_arg_text, replacement) = if bytes[i] == b'"' || bytes[i] == b'\'' {
            // String literal argument
            let quote = bytes[i];
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1; // skip escaped char
                }
                i += 1;
            }
            if i >= len {
                search_from = after_bigint;
                continue;
            }
            i += 1; // skip closing quote
            let arg = &line[arg_start..i];
            let inner = &line[arg_start + 1..i - 1].trim();
            let inner = inner.strip_prefix('+').map(|s| s.trim()).unwrap_or(inner);

            if !is_numeric_arg(inner) {
                search_from = after_bigint;
                continue;
            }

            (arg.to_string(), format!("{}n", inner))
        } else if bytes[i].is_ascii_digit()
            || ((bytes[i] == b'+' || bytes[i] == b'-')
                && i + 1 < len
                && bytes[i + 1].is_ascii_digit())
        {
            // Numeric literal argument (possibly with sign)
            while i < len
                && (bytes[i].is_ascii_hexdigit()
                    || bytes[i] == b'_'
                    || bytes[i] == b'x'
                    || bytes[i] == b'X'
                    || bytes[i] == b'b'
                    || bytes[i] == b'B'
                    || bytes[i] == b'o'
                    || bytes[i] == b'O'
                    || bytes[i] == b'+'
                    || bytes[i] == b'-')
            {
                i += 1;
            }
            let arg = &line[arg_start..i];
            if !is_numeric_arg(arg) {
                search_from = after_bigint;
                continue;
            }
            (arg.to_string(), format!("{}n", arg))
        } else {
            // Variable or expression — skip
            search_from = after_bigint;
            continue;
        };

        // Skip whitespace after argument
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        // Expect ')'
        if i >= len || bytes[i] != b')' {
            search_from = after_bigint;
            continue;
        }
        i += 1; // skip ')'

        let full_match = &line[abs_pos..i];
        results.push((abs_pos, full_match.to_string(), replacement));
        search_from = i;
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

            for (col, full, replacement) in find_bigint_calls(line) {
                if likely_in_string_or_comment(line, col) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "prefer-bigint-literals".into(),
                    message: format!("Prefer `{}` over `{}`.", replacement, full),
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
    fn flags_bigint_with_decimal() {
        let d = run("const x = BigInt(123);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("123n"));
    }

    #[test]
    fn flags_bigint_with_hex() {
        let d = run("const x = BigInt(0xFF);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFFn"));
    }

    #[test]
    fn flags_bigint_with_string() {
        let d = run(r#"const x = BigInt("9007199254740991");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("9007199254740991n"));
    }

    #[test]
    fn flags_bigint_with_large_number() {
        let d = run("const x = BigInt(9007199254740991);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_bigint_literal() {
        assert!(run("const x = 123n;").is_empty());
    }

    #[test]
    fn allows_bigint_with_variable() {
        assert!(run("const x = BigInt(y);").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// BigInt(123)").is_empty());
    }

    #[test]
    fn ignores_strings() {
        assert!(run(r#"const x = "BigInt(123)";"#).is_empty());
    }
}
