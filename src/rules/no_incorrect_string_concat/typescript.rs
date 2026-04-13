//! no-incorrect-string-concat AST backend — flag `"..." + numericVar`.

use crate::diagnostic::{Diagnostic, Severity};

const NUMERIC_HINTS: &[&str] = &[
    "count", "num", "total", "index", "length", "size", "amount", "qty", "sum", "age", "port",
    "offset", "width", "height", "price", "cost",
];

/// Detect `"..." + identifier` patterns where the identifier name suggests a number.
fn has_string_plus_number_var(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            i += 1;

            let rest = &line[i..];
            let trimmed = rest.trim_start();
            if let Some(after_plus) = trimmed.strip_prefix('+') {
                let ident_part = after_plus.trim_start();
                let ident_end = ident_part
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(ident_part.len());
                let ident = &ident_part[..ident_end];
                if !ident.is_empty() {
                    let lower = ident.to_ascii_lowercase();
                    if !ident.starts_with('"') && !ident.starts_with('\'') {
                        for hint in NUMERIC_HINTS {
                            if lower.contains(hint) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if has_string_plus_number_var(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-incorrect-string-concat".into(),
                message: "Suspicious string concatenation with a numeric variable \u{2014} use explicit conversion or template literals.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_string_plus_count() {
        assert_eq!(run_on(r#"const msg = "Total: " + itemCount;"#).len(), 1);
    }

    #[test]
    fn flags_string_plus_total() {
        assert_eq!(run_on(r#"console.log("Sum is " + totalAmount);"#).len(), 1);
    }

    #[test]
    fn allows_string_plus_string_var() {
        assert!(run_on(r#"const msg = "Hello " + userName;"#).is_empty());
    }

    #[test]
    fn allows_template_literal() {
        assert!(run_on(r#"const msg = `Total: ${itemCount}`;"#).is_empty());
    }
}
