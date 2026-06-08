//! comma-or-logical-or-case AST backend — flag `case` clauses that use
//! comma or `||` instead of separate fall-through cases.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["switch_case"] => |node, source, ctx, diagnostics|
    // Get the value field of the case clause.
    let Some(value) = node.child_by_field_name("value") else { return };
    let _value_text = value.utf8_text(source).unwrap_or("");

    // Check for comma-separated values: `case 1, 2:`
    // The tree-sitter grammar parses `case 1, 2:` with a sequence_expression.
    let has_sequence = value.kind() == "sequence_expression";

    // Check for logical OR: `case 1 || 2:`
    let has_logical_or = value.kind() == "binary_expression" && {
        value
            .child_by_field_name("operator")
            .is_some_and(|op| op.utf8_text(source).unwrap_or("") == "||")
    };

    if has_sequence || has_logical_or {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "comma-or-logical-or-case".into(),
            message: "Switch `case` uses comma or `||` — use separate `case` clauses with fall-through instead.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_comma_in_case() {
        let src = r#"switch (x) {
    case 1, 2:
        break;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_logical_or_in_case() {
        let src = r#"switch (x) {
    case 1 || 2:
        break;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_simple_case() {
        let src = r#"switch (x) {
    case 1:
        break;
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fallthrough_pattern() {
        let src = r#"switch (x) {
    case 1:
    case 2:
        break;
}"#;
        assert!(run_on(src).is_empty());
    }
}
