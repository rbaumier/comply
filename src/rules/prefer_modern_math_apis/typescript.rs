//! prefer-modern-math-apis — flag legacy math expressions that have
//! modern replacements:
//!   - `Math.log(x) / Math.LN2`   → `Math.log2(x)`
//!   - `Math.log(x) / Math.LN10`  → `Math.log10(x)`
//!   - `Math.log(x) * Math.LOG2E` → `Math.log2(x)`
//!   - `Math.log(x) * Math.LOG10E`→ `Math.log10(x)`
//!   - `Math.sqrt(a**2 + b**2)`   → `Math.hypot(a, b)`
//!
//! Detection: walk `binary_expression` nodes for the log conversions,
//! and `call_expression` for the `Math.sqrt` argument shape.

use crate::diagnostic::{Diagnostic, Severity};

/// True if `node` is `Math.<name>(...)`.
fn is_math_call(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    is_math_member(callee, name, source)
}

/// True if `node` is `Math.<name>` (member expression).
fn is_math_member(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = node.child_by_field_name("object") else { return false };
    if obj.utf8_text(source).unwrap_or("") != "Math" {
        return false;
    }
    let Some(prop) = node.child_by_field_name("property") else { return false };
    prop.utf8_text(source).unwrap_or("") == name
}

fn unwrap(mut n: tree_sitter::Node) -> tree_sitter::Node {
    while matches!(
        n.kind(),
        "parenthesized_expression"
            | "non_null_expression"
            | "as_expression"
            | "satisfies_expression"
            | "type_assertion"
    ) {
        let Some(c) = n.named_child(0) else { break };
        n = c;
    }
    n
}

/// If the binary_expression is one of the log-conversion shapes, return
/// the suggestion message.
fn log_violation_message(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<&'static str> {
    let op_node = node.child_by_field_name("operator")?;
    let op = op_node.utf8_text(source).ok()?;
    let left = unwrap(node.child_by_field_name("left")?);
    let right = unwrap(node.child_by_field_name("right")?);

    match op {
        "/" => {
            if !is_math_call(left, "log", source) {
                return None;
            }
            if is_math_member(right, "LN2", source) {
                Some("Prefer `Math.log2(x)` over `Math.log(x) / Math.LN2`.")
            } else if is_math_member(right, "LN10", source) {
                Some("Prefer `Math.log10(x)` over `Math.log(x) / Math.LN10`.")
            } else {
                None
            }
        }
        "*" => {
            // Either side may carry the Math.log call.
            let other = if is_math_call(left, "log", source) {
                right
            } else if is_math_call(right, "log", source) {
                left
            } else {
                return None;
            };
            if is_math_member(other, "LOG2E", source) {
                Some("Prefer `Math.log2(x)` over `Math.log(x) * Math.LOG2E`.")
            } else if is_math_member(other, "LOG10E", source) {
                Some("Prefer `Math.log10(x)` over `Math.log(x) * Math.LOG10E`.")
            } else {
                None
            }
        }
        _ => None,
    }
}

/// True if `node` is `<expr> ** 2`.
fn is_squared(node: tree_sitter::Node, source: &[u8]) -> bool {
    let n = unwrap(node);
    if n.kind() != "binary_expression" {
        return false;
    }
    let op = n
        .child_by_field_name("operator")
        .and_then(|o| o.utf8_text(source).ok())
        .unwrap_or("");
    if op != "**" {
        return false;
    }
    let Some(right) = n.child_by_field_name("right") else { return false };
    let r = unwrap(right);
    r.kind() == "number" && r.utf8_text(source).unwrap_or("") == "2"
}

/// True if `node` is `a * a` shape (same identifier on both sides).
fn is_self_mul(node: tree_sitter::Node, source: &[u8]) -> bool {
    let n = unwrap(node);
    if n.kind() != "binary_expression" {
        return false;
    }
    let op = n
        .child_by_field_name("operator")
        .and_then(|o| o.utf8_text(source).ok())
        .unwrap_or("");
    if op != "*" {
        return false;
    }
    let Some(left) = n.child_by_field_name("left") else { return false };
    let Some(right) = n.child_by_field_name("right") else { return false };
    let lt = unwrap(left).utf8_text(source).unwrap_or("");
    let rt = unwrap(right).utf8_text(source).unwrap_or("");
    !lt.is_empty() && lt == rt
}

/// True if `expr` looks like `square + square + ...` where each term is
/// `x ** 2` or `x * x`. At least two terms required.
fn is_sum_of_squares(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Collect `+`-joined terms.
    let mut terms: Vec<tree_sitter::Node> = Vec::new();
    fn collect<'t>(
        n: tree_sitter::Node<'t>,
        source: &[u8],
        out: &mut Vec<tree_sitter::Node<'t>>,
    ) {
        let n = unwrap(n);
        if n.kind() == "binary_expression"
            && n.child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                == Some("+")
        {
            if let Some(l) = n.child_by_field_name("left") {
                collect(l, source, out);
            }
            if let Some(r) = n.child_by_field_name("right") {
                collect(r, source, out);
            }
        } else {
            out.push(n);
        }
    }
    collect(node, source, &mut terms);
    if terms.len() < 2 {
        return false;
    }
    terms.iter().all(|t| is_squared(*t, source) || is_self_mul(*t, source))
}

crate::ast_check! { on ["binary_expression", "call_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        "binary_expression" => {
            if let Some(msg) = log_violation_message(node, source) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    "prefer-modern-math-apis",
                    msg.into(),
                    Severity::Warning,
                ));
            }
        }
        "call_expression" => {
            if !is_math_call(node, "sqrt", source) {
                return;
            }
            let Some(args) = node.child_by_field_name("arguments") else { return };
            // arguments node holds `(` <expr> `)`. Get the first named child.
            let Some(arg) = args.named_child(0) else { return };
            if is_sum_of_squares(arg, source) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    "prefer-modern-math-apis",
                    "Prefer `Math.hypot(a, b)` over `Math.sqrt(a**2 + b**2)`.".into(),
                    Severity::Warning,
                ));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_log_div_ln2() {
        let d = run_ts("const x = Math.log(n) / Math.LN2;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-modern-math-apis");
    }

    #[test]
    fn flags_log_div_ln10() {
        let d = run_ts("const x = Math.log(n) / Math.LN10;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_sqrt_sum_of_squares() {
        let d = run_ts("const h = Math.sqrt(a ** 2 + b ** 2);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_math_log2() {
        assert!(run_ts("const x = Math.log2(n);").is_empty());
    }

    #[test]
    fn allows_math_hypot() {
        assert!(run_ts("const h = Math.hypot(a, b);").is_empty());
    }

    #[test]
    fn allows_plain_math_sqrt() {
        assert!(run_ts("const r = Math.sqrt(x);").is_empty());
    }
}
