//! no-array-delete backend — flag `delete arr[i]`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["unary_expression"] prefilter = ["delete"] => |node, source, ctx, diagnostics|
    // The operator must be `delete`.
    let op = node.child_by_field_name("operator")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");
    if op != "delete" {
        return;
    }

    // The argument must be a subscript_expression (bracket access).
    let Some(arg) = node.child_by_field_name("argument") else { return };
    if arg.kind() != "subscript_expression" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-array-delete".into(),
        message: "`delete arr[i]` creates a sparse hole — use `arr.splice(i, 1)` instead.".into(),
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
    fn flags_delete_array_element() {
        assert_eq!(run_on("delete arr[0];").len(), 1);
    }

    #[test]
    fn flags_delete_with_variable_index() {
        assert_eq!(run_on("delete items[idx];").len(), 1);
    }

    #[test]
    fn allows_delete_object_property() {
        assert!(run_on("delete obj.prop;").is_empty());
    }

    #[test]
    fn ignores_non_delete_lines() {
        assert!(run_on("const x = arr[0];").is_empty());
    }
}
