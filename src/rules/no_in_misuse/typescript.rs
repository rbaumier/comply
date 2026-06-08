//! no-in-misuse backend — flag `x in arr` where `arr` looks like an array.

use crate::diagnostic::{Diagnostic, Severity};

const ARRAY_HINTS: &[&str] = &[
    "arr", "list", "items", "elements", "values", "entries", "rows", "results",
];

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let Ok(op) = op_node.utf8_text(source) else { return };

    if op != "in" {
        return;
    }

    // Skip `for ... in` — the parent is a for_in_statement.
    if let Some(parent) = node.parent()
        && parent.kind() == "for_in_statement" {
            return;
        }

    let Some(right) = node.child_by_field_name("right") else { return };
    let Ok(rhs_text) = right.utf8_text(source) else { return };

    // Check if the RHS looks like an array.
    let lower = rhs_text.to_ascii_lowercase();
    let looks_like_array = rhs_text.starts_with('[')
        || ARRAY_HINTS.iter().any(|hint| lower.contains(hint));

    if !looks_like_array {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-in-misuse".into(),
        message: "`in` operator checks object keys, not array values — use `.includes()` instead.".into(),
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
    fn flags_in_on_array_name() {
        assert_eq!(run_on("if (\"x\" in myItems) {}").len(), 1);
    }

    #[test]
    fn flags_in_on_arr_suffix() {
        assert_eq!(run_on("if (val in userList) {}").len(), 1);
    }

    #[test]
    fn allows_for_in_loop() {
        assert!(run_on("for (const key in obj) {}").is_empty());
    }

    #[test]
    fn allows_in_on_object() {
        assert!(run_on("if (\"name\" in config) {}").is_empty());
    }
}
