//! no-identical-expressions Rust backend.
//!
//! Flag `expr OP expr` where both sides are identical.

use crate::diagnostic::{Diagnostic, Severity};

const FLAGGED_OPS: &[&str] = &["==", "!=", "&&", "||", "-", "/"];

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let Ok(op) = op_node.utf8_text(source) else { return };

    if !FLAGGED_OPS.contains(&op) {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let Ok(left_text) = left.utf8_text(source) else { return };
    let Ok(right_text) = right.utf8_text(source) else { return };

    // Avoid false positives on single-char tokens for `-` and `/`.
    if (op == "-" || op == "/") && left_text.len() <= 1 {
        return;
    }

    if left_text == right_text {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-identical-expressions".into(),
            message: format!(
                "Identical expression `{}` on both sides of `{}`.",
                left_text, op
            ),
            severity: Severity::Error,
            span: None,
        });
    }
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
    fn flags_identical_eq() {
        let d = run_on("fn f() { if a == a {} }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("=="));
    }

    #[test]
    fn flags_identical_and() {
        let d = run_on("fn f() { let ok = valid && valid; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("&&"));
    }

    #[test]
    fn allows_different_sides() {
        assert!(run_on("fn f() { if a == b {} }").is_empty());
    }

    // diesel test code intentionally uses `value - value` to verify SQL null
    // propagation (issue #1500). `skip_in_test_dir` must suppress the rule there.
    #[test]
    fn skips_identical_operands_in_test_dir() {
        let src = "fn f() { let data = nullable_table.select(value - value).load(connection); }";
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "diesel_tests/tests/expressions/ops.rs",
        );
        assert!(d.is_empty(), "rule must be suppressed in a test directory");
    }

    #[test]
    fn flags_identical_operands_in_non_test_dir() {
        let src = "fn f() { let data = nullable_table.select(value - value).load(connection); }";
        let d = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/ops.rs");
        assert_eq!(d.len(), 1, "rule must still fire outside test directories");
        assert!(d[0].message.contains("-"));
    }
}
