//! prefer-less-than Rust backend — flag `binary_expression` nodes using `>`
//! or `>=` and suggest the equivalent `<` / `<=` form with operands swapped.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let suggested = match op {
        ">" => "<",
        ">=" => "<=",
        _ => return,
    };

    if let Some(rhs) = node.child_by_field_name("right")
        && matches!(rhs.kind(), "integer_literal" | "float_literal"
                    | "string_literal" | "boolean_literal" | "char_literal"
                    | "unary_expression") {
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
    fn flags_greater_than() {
        let d = run_on("fn f(a: i32, b: i32) -> bool { b > a }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
    }

    #[test]
    fn flags_greater_or_equal() {
        let d = run_on("fn f(a: i32, b: i32) -> bool { b >= a }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<=`"));
    }

    #[test]
    fn allows_variable_vs_literal() {
        assert!(run_on("fn f(x: i32) { if x > 0 { g(); } }").is_empty());
        assert!(run_on("fn f(x: f64) { if x >= 1.0 { g(); } }").is_empty());
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
