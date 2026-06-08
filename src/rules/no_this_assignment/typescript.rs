//! no-this-assignment backend — flag `const self = this` and `self = this`.
//!
//! Assigning `this` to a variable (typically `self`, `that`, `_this`) is
//! a pre-arrow-function pattern. Arrow functions capture `this` lexically,
//! making the alias unnecessary.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["variable_declarator", "assignment_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        // `const self = this;` / `let that = this;`
        "variable_declarator" => {
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Some(value_node) = node.child_by_field_name("value") else { return };

            if name_node.kind() != "identifier" { return; }
            if value_node.kind() != "this" { return; }

            let var_name = name_node.utf8_text(source).unwrap_or("");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-this-assignment".into(),
                message: format!("Do not assign `this` to `{var_name}`. Use an arrow function instead."),
                severity: Severity::Warning,
                span: None,
            });
        }
        // `self = this;` (reassignment)
        "assignment_expression" => {
            let Some(left) = node.child_by_field_name("left") else { return };
            let Some(right) = node.child_by_field_name("right") else { return };

            if left.kind() != "identifier" { return; }
            if right.kind() != "this" { return; }

            let var_name = left.utf8_text(source).unwrap_or("");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-this-assignment".into(),
                message: format!("Do not assign `this` to `{var_name}`. Use an arrow function instead."),
                severity: Severity::Warning,
                span: None,
            });
        }
        _ => {}
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
    fn flags_const_self_equals_this() {
        let d = run_on("const self = this;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("self"));
    }

    #[test]
    fn flags_let_that_equals_this() {
        let d = run_on("let that = this;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("that"));
    }

    #[test]
    fn flags_assignment_expression() {
        let d = run_on("let x; x = this;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_normal_assignment() {
        assert!(run_on("const x = 42;").is_empty());
    }

    #[test]
    fn allows_this_member_access() {
        assert!(run_on("const x = this.foo;").is_empty());
    }

    #[test]
    fn flags_var_this_equals_this() {
        let d = run_on("var _this = this;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("_this"));
    }
}
