//! prefer-includes — flag `arr.indexOf(x)`/`str.lastIndexOf(x)` existence
//! checks that should be `.includes(x)`.
//!
//! Detection: walk `binary_expression` nodes whose operator is one of the
//! existence-check shapes (`!== -1`, `!= -1`, `> -1`, `>= 0`, `=== -1`,
//! `== -1`, `< 0`) and whose other operand is a `call_expression` on
//! `.indexOf(...)` or `.lastIndexOf(...)`.

use crate::diagnostic::{Diagnostic, Severity};

/// Strip TS assertion / parenthesised wrappers off an operand so the
/// underlying call_expression is visible.
fn unwrap_expr(mut node: tree_sitter::Node) -> tree_sitter::Node {
    while matches!(
        node.kind(),
        "non_null_expression"
            | "parenthesized_expression"
            | "as_expression"
            | "satisfies_expression"
            | "type_assertion"
    ) {
        let Some(child) = node.named_child(0) else {
            break;
        };
        node = child;
    }
    node
}

/// True if `node` is a call expression on `.indexOf(...)` or `.lastIndexOf(...)`.
fn is_indexof_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    matches!(
        prop.utf8_text(source).unwrap_or(""),
        "indexOf" | "lastIndexOf"
    )
}

/// Return Some(true) if (operand_kind, op, literal_text) is one of the
/// existence-check shapes. `lhs_call` indicates whether the indexOf call
/// was on the left (true) or right (false) of the binary expression.
fn is_existence_check(op: &str, lit: &str, lhs_call: bool) -> bool {
    // Normalize so the call is conceptually on the left:
    //   indexOf(x) !== -1   → op="!==", lit="-1"
    //   -1 !== indexOf(x)   → op="!==", lit="-1" (lhs_call=false)
    // Operators are symmetric for ==/!=/===/!==, but ordering matters for </>/<=/>=.
    matches!(
        (op, lit, lhs_call),
        ("!==", "-1", _)
            | ("!=", "-1", _)
            | ("===", "-1", _)
            | ("==", "-1", _)
            | (">", "-1", true)
            | ("<", "-1", false)
            | (">=", "0", true)
            | ("<=", "0", false)
            | ("<", "0", true)
            | (">", "0", false)
    )
}

/// If `node` is a numeric literal `0` or `-1` (with optional leading `-`),
/// return its canonical text. Handles both `unary_expression(-, 1)` and
/// the bare `number` token "-1" some grammars produce.
fn literal_text(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let n = unwrap_expr(node);
    if n.kind() == "number" {
        let t = n.utf8_text(source).ok()?;
        if t == "0" || t == "-1" {
            return Some(t.to_string());
        }
    }
    if n.kind() == "unary_expression" {
        let op = n.child_by_field_name("operator")?.utf8_text(source).ok()?;
        let arg = n.child_by_field_name("argument")?;
        if op == "-" && arg.kind() == "number" && arg.utf8_text(source).ok()? == "1" {
            return Some("-1".to_string());
        }
    }
    None
}

crate::ast_check! { on ["binary_expression"] prefilter = ["indexOf"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let Some(op) = op_node.utf8_text(source).ok() else { return };
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let l = unwrap_expr(left);
    let r = unwrap_expr(right);

    // Try call on left, literal on right.
    let (call_node, lit_text, lhs_call) =
        if is_indexof_call(l, source) {
            let Some(lit) = literal_text(r, source) else { return };
            (l, lit, true)
        } else if is_indexof_call(r, source) {
            let Some(lit) = literal_text(l, source) else { return };
            (r, lit, false)
        } else {
            return;
        };

    if !is_existence_check(op, &lit_text, lhs_call) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &call_node,
        "prefer-includes",
        "Prefer `.includes(x)` over `.indexOf(x) !== -1` — more readable.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_indexof_not_equal_minus_one() {
        assert_eq!(run_ts("if (arr.indexOf(x) !== -1) {}").len(), 1);
    }

    #[test]
    fn flags_indexof_loose_not_equal() {
        assert_eq!(run_ts("if (arr.indexOf(x) != -1) {}").len(), 1);
    }

    #[test]
    fn flags_indexof_gte_zero() {
        assert_eq!(run_ts("if (arr.indexOf(x) >= 0) {}").len(), 1);
    }

    #[test]
    fn flags_lastindexof() {
        assert_eq!(run_ts("if (str.lastIndexOf(c) !== -1) {}").len(), 1);
    }

    #[test]
    fn allows_includes() {
        assert!(run_ts("if (arr.includes(x)) {}").is_empty());
    }

    #[test]
    fn allows_indexof_other_comparison() {
        assert!(run_ts("if (arr.indexOf(x) === 2) {}").is_empty());
    }
}
