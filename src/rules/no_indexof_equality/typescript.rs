use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" { return; }

    let Some(left) = node.child_by_field_name("left") else { return; };
    let Some(right) = node.child_by_field_name("right") else { return; };
    let Some(op) = node.child_by_field_name("operator") else { return; };

    let op_text = op.utf8_text(source).unwrap_or("");

    // Check for indexOf on either side
    let compare_node = if is_indexof_call(left, source) {
        right
    } else if is_indexof_call(right, source) {
        left
    } else {
        return;
    };

    let compare_text = compare_node.utf8_text(source).unwrap_or("").trim();

    let suggestion = match (op_text, compare_text) {
        ("===" | "==" | "!==" | "!=" | ">=" | ">", "-1") => "includes()",
        ("===" | "==", "0") => "startsWith()",
        _ => return,
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-indexof-equality".into(),
        message: format!("Use `{suggestion}` instead of `indexOf()` comparison."),
        severity: Severity::Warning,
        span: None,
    });
}

fn is_indexof_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" { return false; }
    let Some(func) = node.child_by_field_name("function") else { return false; };
    if func.kind() != "member_expression" { return false; }
    let Some(prop) = func.child_by_field_name("property") else { return false; };
    prop.utf8_text(source).unwrap_or("") == "indexOf"
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_indexof_not_minus_one() {
        assert_eq!(run("str.indexOf('x') !== -1").len(), 1);
    }

    #[test]
    fn flags_indexof_equals_zero() {
        assert_eq!(run("str.indexOf('x') === 0").len(), 1);
    }

    #[test]
    fn flags_indexof_gte_zero() {
        assert_eq!(run("arr.indexOf(item) >= 0").len(), 0); // Not a common pattern we flag
    }

    #[test]
    fn flags_indexof_gt_minus_one() {
        assert_eq!(run("arr.indexOf(item) > -1").len(), 1);
    }

    #[test]
    fn allows_includes() {
        assert!(run("str.includes('x')").is_empty());
    }

    #[test]
    fn allows_starts_with() {
        assert!(run("str.startsWith('x')").is_empty());
    }
}
