use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Pattern: [...arr].reverse() or arr.slice().reverse()
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "reverse" { return; }

    let Some(obj) = func.child_by_field_name("object") else { return; };

    let is_copy_pattern = match obj.kind() {
        // [...arr].reverse()
        "array" => {
            let child_count = obj.named_child_count();
            child_count == 1 && obj.named_child(0).map(|c| c.kind()) == Some("spread_element")
        }
        // arr.slice().reverse()
        "call_expression" => {
            if let Some(inner_func) = obj.child_by_field_name("function") {
                if inner_func.kind() == "member_expression" {
                    if let Some(inner_prop) = inner_func.child_by_field_name("property") {
                        inner_prop.utf8_text(source).unwrap_or("") == "slice"
                    } else { false }
                } else { false }
            } else { false }
        }
        _ => false,
    };

    if !is_copy_pattern { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-array-to-reversed".into(),
        message: "Use `arr.toReversed()` instead of copying then reversing (ES2023).".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_spread_reverse() {
        assert_eq!(run("[...arr].reverse()").len(), 1);
    }

    #[test]
    fn flags_slice_reverse() {
        assert_eq!(run("arr.slice().reverse()").len(), 1);
    }

    #[test]
    fn allows_to_reversed() {
        assert!(run("arr.toReversed()").is_empty());
    }

    #[test]
    fn allows_mutating_reverse() {
        assert!(run("arr.reverse()").is_empty());
    }
}
