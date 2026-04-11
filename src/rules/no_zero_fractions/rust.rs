//! no-zero-fractions Rust backend — flag `1.0`, `2.00` float literals
//! where the fractional part is all zeros.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "float_literal" {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");

    // Must contain a dot.
    let Some(dot_pos) = text.find('.') else { return };

    // Skip range operator (shouldn't appear in a float_literal).
    if text.get(dot_pos + 1..dot_pos + 2) == Some(".") {
        return;
    }

    // Strip any type suffix (f32, f64).
    let after_dot = &text[dot_pos + 1..];
    let fraction = after_dot
        .trim_end_matches("f32")
        .trim_end_matches("f64")
        .trim_end_matches('_');

    // Dangling dot: `1.` — fraction is empty.
    let is_dangling = fraction.is_empty();

    // Zero fraction: `1.0`, `1.00`, `1.0_0` — fraction is all zeros/underscores.
    let is_zero_fraction =
        !is_dangling && fraction.chars().all(|c| c == '0' || c == '_');

    if !is_dangling && !is_zero_fraction {
        return;
    }

    let msg = if is_dangling {
        "Don't use a dangling dot in the number."
    } else {
        "Don't use a zero fraction in the number."
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-zero-fractions".into(),
        message: msg.into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_zero_fraction() {
        let d = run_on("fn f() { let x = 1.0; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("zero fraction"));
    }

    #[test]
    fn flags_multiple_trailing_zeros() {
        assert_eq!(run_on("fn f() { let x = 1.00; }").len(), 1);
    }

    #[test]
    fn allows_real_fraction() {
        assert!(run_on("fn f() { let x = 1.5; }").is_empty());
    }

    #[test]
    fn allows_non_zero_fraction() {
        assert!(run_on("fn f() { let x = 3.14; }").is_empty());
    }
}
