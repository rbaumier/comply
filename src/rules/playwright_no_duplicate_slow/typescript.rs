//! playwright-no-duplicate-slow backend — flag repeated `test.slow()` calls
//! within the same test function scope.
//!
//! Why: `test.slow()` doesn't compound. A second call is a copy/paste bug
//! that wastes a line and confuses readers into thinking the timeout is
//! being extended further.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // scope node id → list of `test.slow()` call_expression nodes inside it
    let mut by_scope: HashMap<usize, Vec<tree_sitter::Node>> = HashMap::new();

    // Manual pre-order walk of all descendants.
    let mut cursor = node.walk();
    let mut progressed = cursor.goto_first_child();
    while progressed {
        let child = cursor.node();
        if !(child.is_error() || child.is_missing())
            && child.kind() == "call_expression"
            && is_test_slow_call(child, source)
            && let Some(scope) = enclosing_function_scope(child)
        {
            by_scope.entry(scope.id()).or_default().push(child);
        }

        if !(child.is_error() || child.is_missing()) && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                progressed = false;
                break;
            }
            if cursor.node().id() == node.id() {
                progressed = false;
                break;
            }
        }
    }

    for calls in by_scope.values() {
        // Flag the 2nd and subsequent occurrences.
        for dup in calls.iter().skip(1) {
            let pos = dup.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "playwright-no-duplicate-slow".into(),
                message: "`test.slow()` is already called in this test; remove the duplicate."
                    .into(),
                severity: Severity::Warning,
                span: Some((dup.byte_range().start, dup.byte_range().len())),
            });
        }
    }
}

/// Returns true if `call` is `test.slow()` with no arguments.
fn is_test_slow_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    if obj.kind() != "identifier" || &source[obj.byte_range()] != b"test" {
        return false;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    if prop.kind() != "property_identifier" || &source[prop.byte_range()] != b"slow" {
        return false;
    }
    let arg_count = call
        .child_by_field_name("arguments")
        .map_or(0, |a| a.named_child_count());
    arg_count == 0
}

/// Walk up parents until we hit a function-like scope. Returns `None` if
/// the call is at the top level.
fn enclosing_function_scope(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = node.parent();
    while let Some(n) = current {
        match n.kind() {
            "arrow_function"
            | "function_expression"
            | "function_declaration"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration" => return Some(n),
            _ => {}
        }
        current = n.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_slow() {
        let src = r#"
test('my test', () => {
  test.slow();
  test.slow();
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_single_slow() {
        let src = r#"
test('my test', () => {
  test.slow();
  expect(1).toBe(1);
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_slow_in_different_tests() {
        let src = r#"
test('test1', () => { test.slow(); });
test('test2', () => { test.slow(); });
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_triple_slow() {
        let src = r#"
test('my test', () => {
  test.slow();
  test.slow();
  test.slow();
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_no_slow() {
        assert!(run_on("test('test', () => { expect(1).toBe(1); });").is_empty());
    }
}
