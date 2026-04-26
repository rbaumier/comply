//! no-delete backend — flag the `delete` operator.
//!
//! `delete obj.prop` mutates the target in place and can deoptimize the
//! underlying object shape. Functional code should produce a new object
//! without the property instead.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["unary_expression"] => |node, source, ctx, diagnostics|
    let Some(op) = node.child_by_field_name("operator") else { return };
    let Ok(text) = op.utf8_text(source) else { return };
    if text != "delete" {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-delete",
        "`delete` mutates the target object — return a new object without the property instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_delete_property() {
        assert_eq!(run_on("delete obj.prop;").len(), 1);
    }

    #[test]
    fn flags_delete_computed_property() {
        assert_eq!(run_on("delete obj[key];").len(), 1);
    }

    #[test]
    fn allows_rest_destructuring() {
        assert!(run_on("const { a, ...rest } = obj;").is_empty());
    }

    #[test]
    fn ignores_other_unary_operators() {
        assert!(run_on("const x = !flag;").is_empty());
        assert!(run_on("const y = -value;").is_empty());
        assert!(run_on("typeof x;").is_empty());
    }
}
