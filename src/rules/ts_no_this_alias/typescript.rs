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
            path: ctx.path.to_path_buf(),
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
            path: ctx.path.to_path_buf(),
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
