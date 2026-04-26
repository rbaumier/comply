//! no-negation-in-equality-check — flag `!x === y` (precedence bug).
//!
//! `!x === y` is parsed as `(!x) === y`, not `!(x === y)`.
//! This rule flags any binary equality expression whose left operand
//! is a `!` unary expression (but not double-negated `!!x`).

use crate::diagnostic::{Diagnostic, Severity};

const EQUALITY_OPS: &[&str] = &["===", "!==", "==", "!="];

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let op_node = match node.child_by_field_name("operator") {
        Some(o) => o,
        None => return,
    };
    let op = op_node.utf8_text(source).unwrap_or("");
    if !EQUALITY_OPS.contains(&op) {
        return;
    }

    let left = match node.child_by_field_name("left") {
        Some(l) => l,
        None => return,
    };

    // Left must be a `!expr` unary expression.
    if left.kind() != "unary_expression" {
        return;
    }
    let bang = left
        .child_by_field_name("operator")
        .and_then(|o| o.utf8_text(source).ok())
        .unwrap_or("");
    if bang != "!" {
        return;
    }

    // Exclude double-negation `!!x === y` — the inner argument is also a `!`.
    if let Some(arg) = left.child_by_field_name("argument")
        && arg.kind() == "unary_expression" {
            let inner_op = arg
                .child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            if inner_op == "!" {
                return;
            }
        }

    let pos = left.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-negation-in-equality-check".into(),
        message: format!(
            "Negated expression in equality check: `!x {op} y` is `(!x) {op} y`. \
             Use `x {neg_op} y` or `!(x {op} y)` instead.",
            op = op,
            neg_op = negate_op(op),
        ),
        severity: Severity::Error,
        span: None,
    });
}

fn negate_op(op: &str) -> &str {
    match op {
        "===" => "!==",
        "!==" => "===",
        "==" => "!=",
        "!=" => "==",
        _ => op,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bang_strict_equals() {
        let d = run_on("if (!x === true) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!x === y"));
    }

    #[test]
    fn flags_bang_loose_equals() {
        let d = run_on("if (!x == true) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bang_strict_not_equals() {
        let d = run_on("if (!x !== false) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bang_loose_not_equals() {
        let d = run_on("if (!x != false) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_double_negation() {
        // `!!x === true` is intentional boolean coercion.
        assert!(run_on("if (!!x === true) {}").is_empty());
    }

    #[test]
    fn allows_normal_equality() {
        assert!(run_on("if (x === true) {}").is_empty());
    }

    #[test]
    fn allows_negation_on_right() {
        // Only left-side negation is the precedence bug.
        assert!(run_on("if (x === !y) {}").is_empty());
    }
}
