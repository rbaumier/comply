//! prefer-bigint-literals AST backend — flag `BigInt(123)` and `BigInt("123")`.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a string (without quotes) represents a numeric value valid for BigInt.
fn is_numeric_arg(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let s = s.trim();
    let s = s
        .strip_prefix('+')
        .or_else(|| s.strip_prefix('-'))
        .unwrap_or(s);
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    if s.len() >= 2 {
        let prefix = &s[..2].to_lowercase();
        if prefix == "0x" || prefix == "0b" || prefix == "0o" {
            return s[2..].chars().all(|c| c.is_ascii_hexdigit() || c == '_');
        }
    }
    s.chars().all(|c| c.is_ascii_digit() || c == '_')
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    if callee.utf8_text(source).unwrap_or("") != "BigInt" {
        return;
    }

    // Get the single argument.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let real_args: Vec<_> = args.children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
        .collect();
    if real_args.len() != 1 {
        return;
    }

    let arg = real_args[0];
    let arg_text = arg.utf8_text(source).unwrap_or("");

    let replacement = match arg.kind() {
        "number" => {
            if !is_numeric_arg(arg_text) { return; }
            format!("{}n", arg_text)
        }
        "unary_expression" => {
            // Handle `-123` or `+123`.
            if !is_numeric_arg(arg_text) { return; }
            format!("{}n", arg_text)
        }
        "string" => {
            // Strip quotes and check.
            let inner = arg_text.trim_matches(|c| c == '\'' || c == '"');
            let inner = inner.trim();
            let inner = inner.strip_prefix('+').map(|s| s.trim()).unwrap_or(inner);
            if !is_numeric_arg(inner) { return; }
            format!("{}n", inner)
        }
        _ => return,
    };

    let full = node.utf8_text(source).unwrap_or("");
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-bigint-literals".into(),
        message: format!("Prefer `{}` over `{}`.", replacement, full),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bigint_with_decimal() {
        let d = run_on("const x = BigInt(123);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("123n"));
    }

    #[test]
    fn flags_bigint_with_hex() {
        let d = run_on("const x = BigInt(0xFF);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFFn"));
    }

    #[test]
    fn flags_bigint_with_string() {
        let d = run_on(r#"const x = BigInt("9007199254740991");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("9007199254740991n"));
    }

    #[test]
    fn allows_bigint_literal() {
        assert!(run_on("const x = 123n;").is_empty());
    }

    #[test]
    fn allows_bigint_with_variable() {
        assert!(run_on("const x = BigInt(y);").is_empty());
    }
}
