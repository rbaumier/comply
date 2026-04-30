//! no-typeof-undefined backend — flag `typeof x === 'undefined'`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["binary_expression"] prefilter = ["typeof"] => |node, source, ctx, diagnostics|
    // One side must be a `typeof` unary expression, the other must be "undefined" string.
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    fn typeof_operand<'a>(n: tree_sitter::Node<'a>, source: &[u8]) -> Option<tree_sitter::Node<'a>> {
        if n.kind() != "unary_expression" {
            return None;
        }
        let op = n.child_by_field_name("operator")?;
        if op.utf8_text(source).unwrap_or("") != "typeof" {
            return None;
        }
        n.child_by_field_name("argument")
    }

    let typeof_arg = typeof_operand(left, source).or_else(|| typeof_operand(right, source));
    let Some(arg) = typeof_arg else { return };

    let is_undefined_string = |n: tree_sitter::Node| -> bool {
        if n.kind() != "string" {
            return false;
        }
        let text = n.utf8_text(source).unwrap_or("");
        text == "'undefined'" || text == "\"undefined\""
    };

    if !is_undefined_string(left) && !is_undefined_string(right) {
        return;
    }

    // Only flag when the operand is guaranteed to be a declared binding.
    // `typeof x === 'undefined'` where `x` is a bare identifier is the only
    // safe way to test a possibly-undeclared variable — `x === undefined`
    // throws ReferenceError.
    let safe_to_rewrite = matches!(
        arg.kind(),
        "member_expression" | "subscript_expression"
    );
    if !safe_to_rewrite {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-typeof-undefined".into(),
        message: "Prefer `=== undefined` over `typeof … === 'undefined'` when \
                  the operand is a property access (which cannot throw \
                  ReferenceError).".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_typeof_member_expression() {
        let d =
            crate::rules::test_helpers::run_ts("if (typeof obj.foo === 'undefined') {}", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-typeof-undefined");
    }

    #[test]
    fn flags_typeof_member_expression_double_quotes() {
        let d =
            crate::rules::test_helpers::run_ts(r#"if (typeof obj.foo === "undefined") {}"#, &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_typeof_subscript_expression() {
        let d = crate::rules::test_helpers::run_ts("if (typeof arr[0] === 'undefined') {}", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_typeof_bare_identifier() {
        // `x` may not be declared — `x === undefined` would throw.
        // `typeof x === 'undefined'` is the only safe check.
        let d = crate::rules::test_helpers::run_ts("if (typeof x === 'undefined') {}", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_direct_undefined_comparison() {
        let d = crate::rules::test_helpers::run_ts("if (x === undefined) {}", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_typeof_for_other_types() {
        let d = crate::rules::test_helpers::run_ts("if (typeof x === 'string') {}", &Check);
        assert!(d.is_empty());
    }
}
