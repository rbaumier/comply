//! no-small-switch backend — `switch` with fewer than 3 `case` clauses.
//!
//! Walks `switch_statement` nodes and counts `switch_case` children inside
//! their body. `switch_default` is excluded — only real `case` clauses count.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "switch_statement" {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };

    let mut case_count: usize = 0;
    let named_count = body.named_child_count();
    for i in 0..named_count {
        let Some(child) = body.named_child(i) else { continue };
        if child.kind() == "switch_case" {
            case_count += 1;
        }
    }

    if case_count < 3 {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "no-small-switch",
            format!("`switch` has only {case_count} case(s) — use `if/else` instead."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_switch_with_two_cases() {
        let src = "switch (x) {\n  case 1:\n    break;\n  case 2:\n    break;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-small-switch");
    }

    #[test]
    fn flags_switch_with_one_case() {
        let src = "switch (action.type) {\n  case \"INCREMENT\":\n    return state + 1;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_switch_with_three_cases() {
        let src = "switch (color) {\n  case \"red\":\n    return \"#f00\";\n  case \"green\":\n    return \"#0f0\";\n  case \"blue\":\n    return \"#00f\";\n}";
        assert!(run_on(src).is_empty());
    }
}
