//! de-morgan-simplify Rust backend — flag `!(a && b)` and `!(a || b)`.

use crate::diagnostic::{Diagnostic, Severity};

/// The nearest ancestor of `node` that is not a `parenthesized_expression`,
/// i.e. the effective parent once redundant parentheses are peeled away.
fn effective_parent(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut parent = node.parent();
    while let Some(p) = parent {
        if p.kind() == "parenthesized_expression" {
            parent = p.parent();
        } else {
            return Some(p);
        }
    }
    None
}

/// True when a `!(a && b)` / `!(a || b)` negation sits where distributing De
/// Morgan does not simplify the expression, keyed on the effective parent
/// (parentheses peeled). Two such positions:
///
/// - inner of a double negation (`!!(a && b)`): the effective parent is another
///   `!` unary_expression, so distributing leaves the outer `!` in place and
///   yields `!(!a || !b)`, no simpler than the original.
/// - operand of an enclosing `&&`/`||` (`!(a && b) && c`): the effective parent
///   is a `&&`/`||` binary_expression, so the distributed form `(!a || !b) && c`
///   needs a new pair of parentheses to preserve precedence and grows the
///   operator count instead of shrinking it.
fn is_in_non_simplifying_position(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = effective_parent(node) else {
        return false;
    };
    match parent.kind() {
        "unary_expression" => parent
            .child(0)
            .is_some_and(|op| op.utf8_text(source).unwrap_or("") == "!"),
        "binary_expression" => {
            let Some(op) = parent.child_by_field_name("operator") else {
                return false;
            };
            let op_text = &source[op.byte_range()];
            op_text == b"&&" || op_text == b"||"
        }
        _ => false,
    }
}

crate::ast_check! { on ["unary_expression"] => |node, source, ctx, diagnostics|
    // In tree-sitter-rust, unary_expression has no fields:
    // child(0) = operator ("!"), named_child(0) = operand.
    let Some(op_node) = node.child(0) else { return };
    if op_node.utf8_text(source).unwrap_or("") != "!" {
        return;
    }

    let Some(arg) = node.named_child(0) else { return };

    // In Rust, `!(a && b)` parses as unary_expression whose operand is
    // a parenthesized_expression containing a binary_expression.
    if arg.kind() != "parenthesized_expression" {
        return;
    }

    // parenthesized_expression also has no fields, use named_child(0).
    let Some(inner) = arg.named_child(0) else { return };
    if inner.kind() != "binary_expression" {
        return;
    }
    let Some(bin_op) = inner.child_by_field_name("operator") else { return };
    let bin_op_text = &source[bin_op.byte_range()];
    if bin_op_text != b"&&" && bin_op_text != b"||" {
        return;
    }

    if is_in_non_simplifying_position(node, source) {
        return;
    }

    let pos = node.start_position();
    let op_str = std::str::from_utf8(bin_op_text).unwrap_or("??");
    let suggested = if op_str == "&&" { "||" } else { "&&" };
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "de-morgan-simplify".into(),
        message: format!(
            "Apply De Morgan's law: `!(a {op_str} b)` simplifies to `!a {suggested} !b`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_negated_and() {
        let d = run_on("fn f(a: bool, b: bool) { if !(a && b) {} }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!a || !b"));
    }

    #[test]
    fn flags_negated_or() {
        let d = run_on("fn f(a: bool, b: bool) { if !(a || b) {} }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!a && !b"));
    }

    #[test]
    fn allows_simple_negation() {
        assert!(run_on("fn f(a: bool) { if !a {} }").is_empty());
    }

    #[test]
    fn allows_negated_comparison() {
        assert!(run_on("fn f(a: i32, b: i32) { if !(a == b) {} }").is_empty());
    }

    #[test]
    fn allows_inner_of_double_negation_and() {
        assert!(run_on("fn f(a: bool, b: bool) { let _ = !!(a && b); }").is_empty());
    }

    #[test]
    fn allows_inner_of_double_negation_or() {
        assert!(run_on("fn f(a: bool, b: bool) { let _ = !!(a || b); }").is_empty());
    }

    #[test]
    fn allows_inner_negation_across_parens() {
        // `!(!(a && b))`: a parenthesized_expression sits between the two `!`.
        assert!(run_on("fn f(a: bool, b: bool) { let _ = !(!(a && b)); }").is_empty());
    }

    #[test]
    fn allows_negation_as_left_operand_of_and() {
        // `!(a && b)` is the LEFT operand of an enclosing `&&`: distributing
        // needs new parentheses (`(!a || !b) && c`), so it does not simplify.
        assert!(
            run_on("fn f(a: bool, b: bool, c: bool) { if !(a && b) && c {} }").is_empty()
        );
    }

    #[test]
    fn allows_negation_as_operand_of_or() {
        // `!(a || b)` as an operand of an enclosing `||`.
        assert!(
            run_on("fn f(a: bool, b: bool, c: bool) { if c || !(a || b) {} }").is_empty()
        );
    }

    #[test]
    fn flags_standalone_negation_in_condition() {
        // A standalone `!(a && b)` condition is the rule's legitimate target.
        let d = run_on("fn f(a: bool, b: bool) { if !(a && b) { let _ = 1; } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!a || !b"));
    }

    #[test]
    fn flags_standalone_negation_with_paths() {
        // `if !(passthrough_nullable && B::FOR_FACTORY)` — standalone, still fires.
        let d = run_on(
            "fn f<B>(passthrough_nullable: bool) { if !(passthrough_nullable && B::FOR_FACTORY) {} }",
        );
        assert_eq!(d.len(), 1);
    }
}
