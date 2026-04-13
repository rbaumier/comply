//! no-typeof-undefined backend — flag `typeof x === 'undefined'`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" {
        return;
    }

    // One side must be a `typeof` unary expression, the other must be "undefined" string.
    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let has_typeof = left.kind() == "unary_expression"
        && left.child_by_field_name("operator")
            .is_some_and(|op| op.utf8_text(source).unwrap_or("") == "typeof")
        || right.kind() == "unary_expression"
        && right.child_by_field_name("operator")
            .is_some_and(|op| op.utf8_text(source).unwrap_or("") == "typeof");

    if !has_typeof {
        return;
    }

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

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-typeof-undefined".into(),
        message: "Compare with `undefined` directly instead of using `typeof`. \
                  Replace `typeof x === 'undefined'` with `x === undefined`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_typeof_triple_equals_single_quotes() {
        let d = crate::rules::test_helpers::run_ts(
            "if (typeof x === 'undefined') {}", &Check,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-typeof-undefined");
    }

    #[test]
    fn flags_typeof_double_quotes() {
        let d = crate::rules::test_helpers::run_ts(
            r#"if (typeof x === "undefined") {}"#, &Check,
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_undefined_comparison() {
        let d = crate::rules::test_helpers::run_ts(
            "if (x === undefined) {}", &Check,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_typeof_for_other_types() {
        let d = crate::rules::test_helpers::run_ts(
            "if (typeof x === 'string') {}", &Check,
        );
        assert!(d.is_empty());
    }
}
