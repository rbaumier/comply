//! prefer-less-than Rust backend — flag `binary_expression` nodes using `>`
//! or `>=` whose left operand is a literal or named constant, and suggest the
//! equivalent `<` / `<=` form with operands swapped. Inversion only improves
//! readability for Yoda-style comparisons (constant on the left); when the left
//! operand is a variable or computed expression, `a > b` already reads naturally.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// True when `expr` is a Yoda-style left operand: a literal value or a named
/// constant (SCREAMING_SNAKE_CASE identifier, possibly the final segment of a
/// path). For these, inverting `a > b` to `b < a` puts the variable first and
/// reads more naturally.
fn is_literal_or_constant_left(expr: Node, source: &[u8]) -> bool {
    match expr.kind() {
        "integer_literal" | "float_literal" | "string_literal" | "boolean_literal"
        | "char_literal" => true,
        // `-1`, `!FLAG` etc. — a unary applied to a literal/constant reads as a
        // value. tree-sitter-rust gives the operand as the last (unnamed) child.
        "unary_expression" => expr
            .named_child(expr.named_child_count().saturating_sub(1))
            .is_some_and(|operand| is_literal_or_constant_left(operand, source)),
        "identifier" => expr
            .utf8_text(source)
            .is_ok_and(super::is_screaming_snake_case),
        // `module::CONST` — the final path segment determines constness.
        "scoped_identifier" => expr
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .is_some_and(super::is_screaming_snake_case),
        _ => false,
    }
}

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let suggested = match op {
        ">" => "<",
        ">=" => "<=",
        _ => return,
    };

    let Some(lhs) = node.child_by_field_name("left") else { return };
    if !is_literal_or_constant_left(lhs, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-less-than".into(),
        message: format!(
            "Prefer `{suggested}` over `{op}` for readability \u{2014} swap operands and use `{suggested}`."
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
    fn flags_literal_left() {
        let d = run_on("fn f(x: i32) { if 5 > x { g(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
    }

    #[test]
    fn flags_literal_left_greater_or_equal() {
        let d = run_on("fn f(x: i32) { if 5 >= x { g(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<=`"));
    }

    #[test]
    fn flags_constant_left() {
        let d = run_on("fn f(x: usize) { if MAX > x { g(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
    }

    #[test]
    fn flags_scoped_constant_left() {
        let d = run_on("fn f(x: usize) { if limits::MAX_DIFF_LINES > x { g(); } }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_negative_literal_left() {
        let d = run_on("fn f(x: i32) { if -1 > x { g(); } }");
        assert_eq!(d.len(), 1);
    }

    // Issue #1456 regression: a variable/field/method/path subject on the left
    // already reads naturally; inverting would put the constant/computed value
    // first, which is less readable.
    #[test]
    fn allows_method_call_vs_constant() {
        assert!(run_on("fn f() -> bool { self.doc.len_lines() > MAX_DIFF_LINES }").is_empty());
    }

    #[test]
    fn allows_variable_vs_method_call() {
        assert!(
            run_on("fn f(anchor_col: usize, width: usize) { if anchor_col > self.max_diagnostic_start(width) { g(); } }")
                .is_empty()
        );
    }

    #[test]
    fn allows_field_vs_variable() {
        assert!(run_on("fn f(line: usize) { if hunk.end > line { g(); } }").is_empty());
    }

    #[test]
    fn allows_variable_vs_literal() {
        assert!(run_on("fn f(x: i32) { if x > 0 { g(); } }").is_empty());
        assert!(run_on("fn f(x: f64) { if x >= 1.0 { g(); } }").is_empty());
    }

    #[test]
    fn allows_non_constant_identifier_left() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a > b }").is_empty());
        assert!(run_on("fn f(b: i32, a: i32) -> bool { b > a }").is_empty());
    }

    #[test]
    fn allows_less_than() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a < b }").is_empty());
    }

    #[test]
    fn allows_less_or_equal() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a <= b }").is_empty());
    }

    #[test]
    fn allows_equality() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a == b }").is_empty());
    }
}
