//! no-unnecessary-slice-end backend — flag `.slice(x, arr.length)` etc.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if the second argument is an unnecessary end value.
fn is_unnecessary_end(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed == "Infinity"
        || trimmed == "Number.POSITIVE_INFINITY"
        || trimmed.ends_with(".length")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Check that the callee is a member expression with property "slice".
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "slice" {
        return;
    }

    // Check arguments — must have exactly 2 arguments.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let arg_nodes: Vec<_> = args.children(&mut cursor)
        .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
        .collect();

    if arg_nodes.len() != 2 {
        return;
    }

    let second_text = arg_nodes[1].utf8_text(source).unwrap_or("");
    if is_unnecessary_end(second_text) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-unnecessary-slice-end".into(),
            message: "The `end` argument is unnecessary \u{2014} `.slice(start)` already goes to the end.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_slice_with_length() {
        let d = crate::rules::test_helpers::run_ts("arr.slice(2, arr.length);", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unnecessary-slice-end");
    }

    #[test]
    fn flags_slice_with_infinity() {
        let d = crate::rules::test_helpers::run_ts("str.slice(0, Infinity);", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_slice_without_end() {
        let d = crate::rules::test_helpers::run_ts("arr.slice(2);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_slice_with_numeric_end() {
        let d = crate::rules::test_helpers::run_ts("arr.slice(2, 5);", &Check);
        assert!(d.is_empty());
    }
}
