//! no-undefined-assignment backend — flag `= undefined` assignments.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
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

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-undefined-assignment".into(),
        message: "Do not assign `undefined` \u{2014} use `let x;` or `delete obj.prop` instead.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_let_undefined() {
        let d = crate::rules::test_helpers::run_ts("let x = undefined;", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-undefined-assignment");
    }

    #[test]
    fn flags_reassignment_undefined() {
        let d = crate::rules::test_helpers::run_ts("x = undefined;", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_comparison_equals() {
        let d = crate::rules::test_helpers::run_ts("if (x == undefined) {}", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_strict_comparison() {
        let d = crate::rules::test_helpers::run_ts("if (x === undefined) {}", &Check);
        assert!(d.is_empty());
    }
}
