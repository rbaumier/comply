//! de-morgan-simplify Rust backend — flag `!(a && b)` and `!(a || b)`.

use crate::diagnostic::{Diagnostic, Severity};

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
}
