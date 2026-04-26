//! number-literal-case Rust backend — enforce lowercase prefix, uppercase hex digits.

use crate::diagnostic::{Diagnostic, Severity};

/// The canonical form: lowercase prefix/exponent, uppercase hex digits.
fn canonical(raw: &str) -> Option<String> {
    if raw.len() < 2 {
        return None;
    }

    let prefix_lower = raw[..2].to_lowercase();
    let fixed = match prefix_lower.as_str() {
        "0x" => {
            let digits = &raw[2..];
            format!("0x{}", digits.to_uppercase())
        }
        "0b" | "0o" => {
            format!("{}{}", prefix_lower, &raw[2..])
        }
        _ => {
            // Rust doesn't have JS-style exponent notation for integer literals,
            // but scientific notation exists in float contexts. Skip if no match.
            return None;
        }
    };

    if fixed == raw {
        None
    } else {
        Some(fixed)
    }
}

crate::ast_check! { on ["integer_literal"] => |node, source, ctx, diagnostics|
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
    use crate::rules::test_helpers::run_rust;

    #[test]
    fn flags_lowercase_hex_digits() {
        let d = run_rust("fn f() { let x = 0xff; }", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn flags_mixed_case_hex_digits() {
        let d = run_rust("fn f() { let x = 0xfF; }", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn allows_correct_hex() {
        assert!(run_rust("fn f() { let x = 0xFF; }", &Check).is_empty());
    }

    #[test]
    fn allows_correct_binary() {
        assert!(run_rust("fn f() { let x = 0b1010; }", &Check).is_empty());
    }

    #[test]
    fn allows_correct_octal() {
        assert!(run_rust("fn f() { let x = 0o777; }", &Check).is_empty());
    }

    #[test]
    fn allows_plain_integer() {
        assert!(run_rust("fn f() { let x = 42; }", &Check).is_empty());
    }
}
