//! ts-no-this-alias backend — flag `const self = this` and similar.
//!
//! Detection: walk `variable_declarator` and `assignment_expression`
//! nodes where the init/right is `this`. Allow destructuring by default.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind == "variable_declarator" {
        let Some(init) = node.child_by_field_name("value") else {
            return;
        };
        if init.kind() != "this" {
            return;
        }
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        // Allow destructuring: `const { a } = this`
        if name_node.kind() != "identifier" {
            return;
        }
        let pos = name_node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-no-this-alias".into(),
            message: "Unexpected aliasing of `this` to a local variable.".into(),
            severity: Severity::Warning,
            span: None,
        });
    } else if kind == "assignment_expression" {
        let Some(right) = node.child_by_field_name("right") else {
            return;
        };
        if right.kind() != "this" {
            return;
        }
        let Some(left) = node.child_by_field_name("left") else {
            return;
        };
        if left.kind() != "identifier" {
            return;
        }
        let pos = left.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-no-this-alias".into(),
            message: "Unexpected aliasing of `this` to a local variable.".into(),
            severity: Severity::Warning,
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
    fn flags_this_alias_const() {
        let diags = run_on("const self = this;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_this_alias_let() {
        let diags = run_on("let that = this;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_destructuring_this() {
        assert!(run_on("const { a, b } = this;").is_empty());
    }

    #[test]
    fn allows_normal_assignment() {
        assert!(run_on("const x = 42;").is_empty());
    }
}
