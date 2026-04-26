//! no-unnecessary-array-splice-count backend — flag `.splice(x, arr.length)` etc.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if the second argument is an unnecessary count/skip value.
fn is_unnecessary_count(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed == "Infinity"
        || trimmed == "Number.POSITIVE_INFINITY"
        || trimmed.ends_with(".length")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Check that the callee is a member expression with property "splice" or "toSpliced".
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if method != "splice" && method != "toSpliced" {
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
    if is_unnecessary_count(second_text) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-unnecessary-array-splice-count".into(),
            message: "The count argument is unnecessary \u{2014} `.splice(start)` already removes all elements from `start`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_splice_with_length() {
        let d = crate::rules::test_helpers::run_ts("arr.splice(2, arr.length);", &Check);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unnecessary-array-splice-count");
    }

    #[test]
    fn flags_splice_with_infinity() {
        let d = crate::rules::test_helpers::run_ts("arr.splice(0, Infinity);", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_to_spliced_with_length() {
        let d = crate::rules::test_helpers::run_ts("arr.toSpliced(2, arr.length);", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_splice_without_count() {
        let d = crate::rules::test_helpers::run_ts("arr.splice(2);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_splice_with_numeric_count() {
        let d = crate::rules::test_helpers::run_ts("arr.splice(2, 3);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_splice_with_replacement_items() {
        let d = crate::rules::test_helpers::run_ts("arr.splice(2, arr.length, 'a', 'b');", &Check);
        assert!(d.is_empty());
    }
}
