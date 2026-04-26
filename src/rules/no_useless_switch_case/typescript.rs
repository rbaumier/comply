//! no-useless-switch-case backend — flag empty case clauses that fall
//! through directly to `default`. These cases have no effect since
//! `default` already handles all unmatched values.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["switch_statement"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };

    // Collect all switch_case and switch_default children.
    let mut cases: Vec<tree_sitter::Node> = Vec::new();
    let child_count = body.named_child_count();
    for i in 0..child_count {
        let Some(child) = body.named_child(i) else { continue };
        if child.kind() == "switch_case" || child.kind() == "switch_default" {
            cases.push(child);
        }
    }

    // Must have at least 2 cases and the last one must be `default`.
    if cases.len() < 2 {
        return;
    }
    let last = cases[cases.len() - 1];
    if last.kind() != "switch_default" {
        return;
    }

    // Walk backwards from the case just before `default` and flag
    // empty cases that fall through.
    let mut i = cases.len() - 2;
    loop {
        let case_node = cases[i];
        if case_node.kind() != "switch_case" {
            break;
        }

        // A case is "empty" if it has no consequent statements.
        // In tree-sitter, switch_case has the test as a named child,
        // then consequent statements as further named children.
        // An empty fallthrough case has only the test (value) child.
        let is_empty = is_empty_case(case_node, source);

        if !is_empty {
            break;
        }

        let pos = case_node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-useless-switch-case".into(),
            message: "Useless case in switch statement — it falls through \
                      to `default` with no own code."
                .into(),
            severity: Severity::Warning,
            span: None,
        });

        if i == 0 {
            break;
        }
        i -= 1;
    }
}

/// Check whether a `switch_case` node has no consequent statements
/// (i.e. it only contains the `case X:` header with no body).
fn is_empty_case(case_node: tree_sitter::Node, _source: &[u8]) -> bool {
    // tree-sitter switch_case: first named child is "value" (the test),
    // remaining named children are the consequent statements.
    // If there's only the value child (or value + only comments/empty statements),
    // the case is empty.
    let named_count = case_node.named_child_count();

    // The first named child is the case value.
    if named_count <= 1 {
        return true;
    }

    // Check if all children after the first (value) are empty_statement or comment.
    for i in 1..named_count {
        let Some(child) = case_node.named_child(i) else {
            continue;
        };
        match child.kind() {
            "empty_statement" | "comment" => continue,
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    // ---- flags useless cases ----

    #[test]
    fn flags_single_empty_case_before_default() {
        let src = r#"
switch (x) {
    case 1:
    default:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_empty_cases_before_default() {
        let src = r#"
switch (x) {
    case 1:
    case 2:
    case 3:
    default:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 3);
    }

    // ---- allows correct usage ----

    #[test]
    fn allows_case_with_body() {
        let src = r#"
switch (x) {
    case 1:
        console.log('one');
        break;
    default:
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_no_default() {
        let src = r#"
switch (x) {
    case 1:
    case 2:
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fallthrough_to_case_not_default() {
        let src = r#"
switch (x) {
    case 1:
    case 2:
        console.log('1 or 2');
        break;
    default:
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_only_empty_trailing_cases() {
        // case 1 has a body, case 2 is empty before default
        let src = r#"
switch (x) {
    case 1:
        console.log('one');
        break;
    case 2:
    default:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
