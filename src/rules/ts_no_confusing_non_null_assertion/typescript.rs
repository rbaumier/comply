//! ts-no-confusing-non-null-assertion backend — flag `non_null_expression`
//! nodes whose parent is a binary expression with `==`, `===`, `=`,
//! `instanceof`, or `in` operator.
//!
//! Detection: walk `non_null_expression` nodes and check if they appear
//! as the left side of a comparison/assignment operator.

use crate::diagnostic::{Diagnostic, Severity};

const CONFUSING_OPS: &[&str] = &["==", "===", "=", "instanceof", "in"];

crate::ast_check! { on ["non_null_expression"] => |node, source, ctx, diagnostics|
    let Some(parent) = node.parent() else {
        return;
    };
    // Check if parent is a binary_expression or augmented_assignment
    let parent_kind = parent.kind();
    if parent_kind != "binary_expression" && parent_kind != "assignment_expression" {
        return;
    }
    // Check if this non_null_expression is the left operand
    let Some(left) = parent.child_by_field_name("left") else {
        return;
    };
    if left.id() != node.id() {
        return;
    }
    // Check the operator
    let Some(op_node) = parent.child_by_field_name("operator") else {
        return;
    };
    let op_text = &source[op_node.byte_range()];
    let Ok(op) = std::str::from_utf8(op_text) else {
        return;
    };
    if !CONFUSING_OPS.contains(&op) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-confusing-non-null-assertion".into(),
        message: "Confusing non-null assertion before comparison — \
                  `a! == b` looks like `a !== b`. Remove the `!` or \
                  wrap in parentheses."
            .into(),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_non_null_before_equality() {
        let diags = run_on("const r = a! == b;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_non_null_before_strict_equality() {
        let diags = run_on("const r = a! === b;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_proper_not_equal() {
        assert!(run_on("const r = a !== b;").is_empty());
    }

    #[test]
    fn allows_proper_not_strict_equal() {
        assert!(run_on("const r = a != b;").is_empty());
    }
}
