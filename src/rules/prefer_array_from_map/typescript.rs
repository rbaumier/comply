use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Look for [...iter].map(fn)
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "map" { return; }

    let Some(obj) = func.child_by_field_name("object") else { return; };
    if obj.kind() != "array" { return; }

    // Check if array is [...something]
    if obj.named_child_count() != 1 { return; }
    let Some(child) = obj.named_child(0) else { return; };
    if child.kind() != "spread_element" { return; }

    // Get the spread argument to check it's not already an array
    let Some(spread_arg) = child.named_child(0) else { return; };

    // Skip if spreading an array literal (that would be weird but valid)
    if spread_arg.kind() == "array" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-array-from-map".into(),
        message: "Use `Array.from(iter, mapFn)` instead of `[...iter].map(mapFn)`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_spread_map() {
        assert_eq!(run("[...set].map(x => x * 2)").len(), 1);
        assert_eq!(run("[...iter].map(fn)").len(), 1);
    }

    #[test]
    fn allows_array_from() {
        assert!(run("Array.from(set, x => x * 2)").is_empty());
    }

    #[test]
    fn allows_array_literal_map() {
        assert!(run("[1, 2, 3].map(x => x * 2)").is_empty());
    }

    #[test]
    fn allows_variable_map() {
        assert!(run("arr.map(x => x * 2)").is_empty());
    }
}
