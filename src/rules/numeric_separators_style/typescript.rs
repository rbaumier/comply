//! numeric-separators-style — enforce grouping digits with underscores.

use crate::diagnostic::{Diagnostic, Severity};

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
fn format_decimal(raw: &str, suffix: &str) -> String {
    let clean: String = raw.chars().filter(|&c| c != '_').collect();
    if clean.len() < 5 {
        return format!("{}{}", clean, suffix);
    }
    let formatted = add_separators(raw, 3);
    format!("{}{}", formatted, suffix)
}

/// Compute the expected format for a number literal. Returns `None` if already correct.
fn expected_format(raw: &str) -> Option<String> {
    let (body, suffix) = if let Some(stripped) = raw.strip_suffix('n') {
        (stripped, "n")
    } else {
        (raw, "")
    };

    if body.len() < 2 {
        return None;
    }

    // Check for prefixed literal: 0x, 0b, 0o
    if body.starts_with("0x")
        || body.starts_with("0X")
        || body.starts_with("0b")
        || body.starts_with("0B")
        || body.starts_with("0o")
        || body.starts_with("0O")
    {
        let prefix = &body[..2];
        let digits = &body[2..];
        let formatted = format_prefixed(prefix, digits, suffix);
        if formatted != raw {
            return Some(formatted);
        }
        return None;
    }

    // Skip floats and exponents — they're more complex
    if body.contains('.') || body.contains('e') || body.contains('E') {
        return None;
    }

    // Decimal integer
    let formatted = format_decimal(body, suffix);
    if formatted != raw {
        return Some(formatted);
    }

    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "number" {
        return;
    }

    let raw = node.utf8_text(source).unwrap_or("");
    if let Some(formatted) = expected_format(raw) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "numeric-separators-style".into(),
            message: format!(
                "Invalid group length in numeric value: `{}` should be `{}`.",
                raw, formatted
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_large_decimal_without_separators() {
        let d = run_ts("const x = 1000000;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("1_000_000"));
    }

    #[test]
    fn flags_five_digit_number() {
        let d = run_ts("const x = 10000;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("10_000"));
    }

    #[test]
    fn allows_four_digit_number() {
        assert!(run_ts("const x = 1000;", &Check).is_empty());
    }

    #[test]
    fn allows_already_separated() {
        assert!(run_ts("const x = 1_000_000;", &Check).is_empty());
    }

    #[test]
    fn flags_hex_without_separators() {
        let d = run_ts("const x = 0xFF00FF;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF_00_FF"));
    }

    #[test]
    fn allows_short_hex() {
        assert!(run_ts("const x = 0xFF;", &Check).is_empty());
    }
}
