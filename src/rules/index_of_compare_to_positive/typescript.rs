//! index-of-compare-to-positive backend — `.indexOf(…) < 1` matches index 0 and absence.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["binary_expression"] prefilter = ["indexOf"] => |node, source, ctx, diagnostics|
    // Match binary expressions: `expr < 1`.
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    if op != "<" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let right_text = right.utf8_text(source).unwrap_or("").trim();

    // `.indexOf(…) < 1` is ambiguously `=== 0 || === -1` — a likely forgotten
    // `-1` bug. `> 0` is intentionally excluded: it is a valid "present at a
    // non-leading position" check, not a bug.
    if right_text != "1" {
        return;
    }

    // Check if left side is a `.indexOf(...)` call.
    if left.kind() != "call_expression" {
        return;
    }
    let Some(callee) = left.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let prop_name = prop.utf8_text(source).unwrap_or("");
    if prop_name != "indexOf" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "index-of-compare-to-positive".into(),
        message: "`.indexOf(…) < 1` matches both index 0 and absence — use `< 0` or `!== -1`.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_indexof_gt_zero() {
        // #3840: `> 0` is a valid "present at a non-leading position" check.
        assert!(run_on("if (arr.indexOf(x) > 0) {}").is_empty());
    }

    #[test]
    fn flags_indexof_lt_one() {
        assert_eq!(run_on("if (str.indexOf('a') < 1) {}").len(), 1);
    }

    #[test]
    fn allows_indexof_gte_zero() {
        assert!(run_on("if (arr.indexOf(x) >= 0) {}").is_empty());
    }

    #[test]
    fn allows_indexof_neq_minus_one() {
        assert!(run_on("if (arr.indexOf(x) !== -1) {}").is_empty());
    }
}
