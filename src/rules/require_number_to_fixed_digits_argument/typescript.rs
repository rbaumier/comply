//! require-number-to-fixed-digits-argument AST backend.
//!
//! Flags `.toFixed()` calls with no digits argument.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // callee must be `*.toFixed`
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "toFixed" {
        return;
    }

    // arguments: must have zero arguments
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 0 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "require-number-to-fixed-digits-argument".into(),
        message: "Missing the digits argument in `.toFixed()` \u{2014} use `.toFixed(0)` explicitly.".into(),
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
    fn flags_empty_to_fixed() {
        assert_eq!(run_on("const s = num.toFixed();").len(), 1);
    }

    #[test]
    fn flags_chained_to_fixed() {
        assert_eq!(run_on("price.toFixed().padStart(5)").len(), 1);
    }

    #[test]
    fn allows_to_fixed_with_digits() {
        assert!(run_on("const s = num.toFixed(2);").is_empty());
    }

    #[test]
    fn allows_to_fixed_with_zero() {
        assert!(run_on("const s = num.toFixed(0);").is_empty());
    }
}
