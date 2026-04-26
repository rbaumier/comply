use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Look for Array.from({length: n}, () => value)
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let Some(obj) = func.child_by_field_name("object") else { return; };
    if obj.utf8_text(source).unwrap_or("") != "Array" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "from" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    if args.named_child_count() != 2 { return; }

    // First arg should be {length: n}
    let Some(first_arg) = args.named_child(0) else { return; };
    if first_arg.kind() != "object" { return; }

    let has_length_only = first_arg.named_child_count() == 1
        && first_arg.named_child(0)
            .and_then(|p| p.child_by_field_name("key"))
            .and_then(|k| k.utf8_text(source).ok())
            .map(|k| k == "length")
            .unwrap_or(false);
    if !has_length_only { return; }

    // Second arg should be arrow function returning constant
    let Some(second_arg) = args.named_child(1) else { return; };
    if second_arg.kind() != "arrow_function" { return; }

    let Some(body) = second_arg.child_by_field_name("body") else { return; };

    // Check if body is a simple literal (not using parameters)
    let is_constant = matches!(body.kind(), "number" | "string" | "true" | "false" | "null");
    if !is_constant { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-array-fill".into(),
        message: "Use `Array(n).fill(value)` instead of `Array.from({length: n}, () => value)`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_array_from_constant() {
        assert_eq!(run("Array.from({length: 5}, () => 0)").len(), 1);
        assert_eq!(run("Array.from({length: n}, () => null)").len(), 1);
    }

    #[test]
    fn allows_array_from_with_index() {
        // Uses index parameter, can't use fill
        assert!(run("Array.from({length: 5}, (_, i) => i)").is_empty());
    }

    #[test]
    fn allows_array_fill() {
        assert!(run("Array(5).fill(0)").is_empty());
    }

    #[test]
    fn allows_array_from_iterable() {
        assert!(run("Array.from(set)").is_empty());
    }
}
