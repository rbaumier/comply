//! non-existent-operator — detect typo operators `=+`, `=-`, `=!`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    // We look for assignment_expression nodes where the right side starts
    // with a unary +, -, or ! — this is the AST shape produced by `x =+ 1`,
    // `x =- 1`, `x =! y`.  The user likely meant `+=`, `-=`, `!=`.
    //
    // For `=+` and `=-`: tree-sitter parses `x =+ 1` as an assignment
    // where the RHS is a unary_expression (`+1`).
    // For `=!`: tree-sitter parses `x =! y` as an assignment where the
    // RHS is a unary_expression (`!y`).
    // Must be a plain `=` assignment (not `+=`, `-=`, etc.)
    let Some(op_node) = node.child(1) else { return };
    let op_text = op_node.utf8_text(source).unwrap_or("");
    if op_text != "=" {
        return;
    }

    let Some(rhs) = node.child_by_field_name("right") else { return };
    if rhs.kind() != "unary_expression" {
        return;
    }

    // The unary operator is the first child of the unary_expression.
    let Some(unary_op) = rhs.child(0) else { return };
    let unary_text = unary_op.utf8_text(source).unwrap_or("");
    if unary_text != "+" && unary_text != "-" && unary_text != "!" {
        return;
    }

    // Check adjacency: the `=` and the unary op must be adjacent (no space)
    // to distinguish `x =+1` (typo) from `x = +1` (intentional).
    let eq_end = op_node.end_byte();
    let unary_start = unary_op.start_byte();
    if unary_start != eq_end {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "non-existent-operator".into(),
        message: "Typo operator — did you mean `+=`, `-=`, or `!=`?".into(),
        severity: Severity::Error,
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
    
    #[test]
    fn flags_equals_plus() {
        let d = crate::rules::test_helpers::run_rule(&Check, "x =+ 1;", "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "non-existent-operator");
    }

    #[test]
    fn flags_equals_minus() {
        let d = crate::rules::test_helpers::run_rule(&Check, "x =- 1;", "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_equals_bang() {
        let d = crate::rules::test_helpers::run_rule(&Check, "x =! true;", "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_plus_equals() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "x += 1;", "t.ts").is_empty());
    }

    #[test]
    fn allows_minus_equals() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "x -= 1;", "t.ts").is_empty());
    }

    #[test]
    fn allows_not_equals() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "if (x !== y) {}", "t.ts").is_empty());
        assert!(crate::rules::test_helpers::run_rule(&Check, "if (x != y) {}", "t.ts").is_empty());
    }

    #[test]
    fn allows_unary_with_space() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "x = +1;", "t.ts").is_empty());
        assert!(crate::rules::test_helpers::run_rule(&Check, "x = -1;", "t.ts").is_empty());
        assert!(crate::rules::test_helpers::run_rule(&Check, "x = !true;", "t.ts").is_empty());
    }
}
