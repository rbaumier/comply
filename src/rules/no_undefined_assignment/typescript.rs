//! no-undefined-assignment backend — flag `= undefined` assignments.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["variable_declarator", "assignment_expression"] prefilter = ["undefined"] => |node, source, ctx, diagnostics|
    // Match variable_declarator or assignment_expression where the value is `undefined`.
    let value_node = match node.kind() {
        "variable_declarator" => node.child_by_field_name("value"),
        "assignment_expression" => node.child_by_field_name("right"),
        _ => return,
    };

    let Some(value) = value_node else { return };

    if value.kind() != "undefined" {
        return;
    }

    // Re-assigning a plain local variable to `undefined` is the only way to reset
    // it: `let x;` is for the declaration and `delete obj.prop` is for properties,
    // so neither remediation applies to a plain-identifier re-assignment target.
    if node.kind() == "assignment_expression"
        && node.child_by_field_name("left").map(|left| left.kind()) == Some("identifier")
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-undefined-assignment".into(),
        message: "Do not assign `undefined` \u{2014} use `let x;` or `delete obj.prop` instead.".into(),
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

    #[test]
    fn flags_let_undefined() {
        let d = crate::rules::test_helpers::run_rule(&Check, "let x = undefined;", "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-undefined-assignment");
    }

    #[test]
    fn allows_plain_identifier_reassignment() {
        let d = crate::rules::test_helpers::run_rule(&Check, "x = undefined;", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_comparison_equals() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (x == undefined) {}", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_strict_comparison() {
        let d = crate::rules::test_helpers::run_rule(&Check, "if (x === undefined) {}", "t.ts");
        assert!(d.is_empty());
    }
}
