//! arguments-order AST backend — flag calls where `expected` comes before
//! `actual` or `max` before `min`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(args_node) = node.child_by_field_name("arguments") else { return };

    // Collect argument identifiers in order.
    let mut arg_names: Vec<&str> = Vec::new();
    let count = args_node.named_child_count();
    for i in 0..count {
        let child = args_node.named_child(i).unwrap();
        let name = match child.kind() {
            "identifier" => child.utf8_text(source).unwrap_or(""),
            _ => "",
        };
        arg_names.push(name);
    }

    // Check for `expected` before `actual`.
    let exp_pos = arg_names.iter().position(|n| n.contains("expected"));
    let act_pos = arg_names.iter().position(|n| n.contains("actual"));
    let expected_before_actual = matches!((exp_pos, act_pos), (Some(e), Some(a)) if e < a);

    // Check for `max` before `min`.
    let max_pos = arg_names.iter().position(|n| n.contains("max"));
    let min_pos = arg_names.iter().position(|n| n.contains("min"));
    let max_before_min = matches!((max_pos, min_pos), (Some(mx), Some(mn)) if mx < mn);

    if expected_before_actual || max_before_min {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "arguments-order".into(),
            message: "Arguments appear to be in the wrong order — `expected` should come after `actual`, `min` before `max`.".into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_expected_before_actual() {
        assert_eq!(run_on("assertEqual(expected, actual);").len(), 1);
    }

    #[test]
    fn flags_max_before_min() {
        assert_eq!(run_on("clamp(max, min);").len(), 1);
    }

    #[test]
    fn allows_correct_order_actual_expected() {
        assert!(run_on("assertEqual(actual, expected);").is_empty());
    }

    #[test]
    fn allows_correct_order_min_max() {
        assert!(run_on("clamp(min, max);").is_empty());
    }

    #[test]
    fn ignores_non_call_lines() {
        assert!(run_on("// expected before actual").is_empty());
    }
}
