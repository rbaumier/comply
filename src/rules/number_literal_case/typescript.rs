//! number-literal-case — enforce lowercase prefix/exponent, uppercase hex digits.

use crate::diagnostic::{Diagnostic, Severity};

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
            let digits = &body[2..];
            format!("0x{}{}", digits.to_uppercase(), suffix)
        }
        "0b" | "0o" => {
            format!("{}{}{}", prefix_lower, &body[2..], suffix)
        }
        _ => {
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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "number" {
        return;
    }

    let raw = node.utf8_text(source).unwrap_or("");
    if let Some(fixed) = canonical(raw) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "number-literal-case".into(),
            message: format!(
                "Invalid number literal casing: `{}` should be `{}`.",
                raw, fixed
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
    fn flags_uppercase_hex_prefix() {
        let d = run_ts("const x = 0XFF;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn flags_lowercase_hex_digits() {
        let d = run_ts("const x = 0xff;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn flags_uppercase_exponent() {
        let d = run_ts("const x = 1E3;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("1e3"));
    }

    #[test]
    fn flags_uppercase_binary_prefix() {
        let d = run_ts("const x = 0B1010;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0b1010"));
    }

    #[test]
    fn flags_uppercase_octal_prefix() {
        let d = run_ts("const x = 0O777;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0o777"));
    }

    #[test]
    fn allows_correct_hex() {
        assert!(run_ts("const x = 0xFF;", &Check).is_empty());
    }

    #[test]
    fn allows_correct_exponent() {
        assert!(run_ts("const x = 1e3;", &Check).is_empty());
    }

    #[test]
    fn allows_correct_binary() {
        assert!(run_ts("const x = 0b1010;", &Check).is_empty());
    }

    #[test]
    fn flags_bigint_hex() {
        let d = run_ts("const x = 0XFFn;", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFFn"));
    }
}
