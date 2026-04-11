use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `"..." + identifier` patterns where the identifier name suggests a
/// number (e.g. contains "count", "num", "total", "index", "length", "size",
/// "amount", "qty", "sum", "id", "age", "port", "offset", "width", "height").
fn has_string_plus_number_var(line: &str) -> bool {
    const NUMERIC_HINTS: &[&str] = &[
        "count", "num", "total", "index", "length", "size", "amount", "qty", "sum", "age", "port",
        "offset", "width", "height", "price", "cost",
    ];

    // Pattern: "..." + someVar
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find a string literal (single or double quote)
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            // Skip string content
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1; // skip escaped char
                }
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            i += 1; // skip closing quote

            // Look for ` + identifier` after the string
            let rest = &line[i..];
            let trimmed = rest.trim_start();
            if let Some(after_plus) = trimmed.strip_prefix('+') {
                let ident_part = after_plus.trim_start();
                // Extract identifier
                let ident_end = ident_part
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(ident_part.len());
                let ident = &ident_part[..ident_end];
                if !ident.is_empty() {
                    let lower = ident.to_ascii_lowercase();
                    // Not a string literal itself
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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_string_plus_number_var(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-incorrect-string-concat".into(),
                    message: "Suspicious string concatenation with a numeric variable \u{2014} use explicit conversion or template literals.".into(),
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
    fn flags_string_plus_count() {
        assert_eq!(run(r#"const msg = "Total: " + itemCount;"#).len(), 1);
    }

    #[test]
    fn flags_string_plus_total() {
        assert_eq!(run(r#"console.log("Sum is " + totalAmount);"#).len(), 1);
    }

    #[test]
    fn allows_string_plus_string_var() {
        assert!(run(r#"const msg = "Hello " + userName;"#).is_empty());
    }

    #[test]
    fn allows_template_literal() {
        assert!(run(r#"const msg = `Total: ${itemCount}`;"#).is_empty());
    }
}
