//! cyclomatic-complexity Rust backend.
//!
//! Count decision points inside each `function_item`: if, match arm,
//! for, while, loop, &&, ||.

use crate::diagnostic::{Diagnostic, Severity};

const THRESHOLD: usize = 10;

const FUNCTION_KINDS: &[&str] = &["function_item"];

const BRANCHING_KINDS: &[&str] = &[
    "if_expression",
    "for_expression",
    "while_expression",
    "loop_expression",
    "match_arm",
];

const LOGICAL_OPS: &[&str] = &["&&", "||"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !FUNCTION_KINDS.contains(&node.kind()) {
        return;
    }

    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");

    let complexity = 1 + count_complexity(node, source);

    if complexity > THRESHOLD {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "cyclomatic-complexity".into(),
            message: format!(
                "Function `{name}` has a cyclomatic complexity of {complexity} (max: {THRESHOLD}).",
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn count_complexity(node: tree_sitter::Node, source: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return 0;
    }
    loop {
        let child = cursor.node();

        // Don't recurse into nested functions.
        if FUNCTION_KINDS.contains(&child.kind()) || child.kind() == "closure_expression" {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        if BRANCHING_KINDS.contains(&child.kind()) {
            count += 1;
        }

        if child.kind() == "binary_expression"
            && let Some(op) = child.child_by_field_name("operator")
        {
            let op_text = op.utf8_text(source).unwrap_or("");
            if LOGICAL_OPS.contains(&op_text) {
                count += 1;
            }
        }

        count += count_complexity(child, source);

        if !cursor.goto_next_sibling() {
            break;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn allows_simple_function() {
        let src = r#"
fn simple(a: bool) -> i32 {
    if a {
        return 1;
    }
    2
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_complex_function() {
        // 1 base + 11 if = 12 complexity
        let src = r#"
fn complex(x: i32) {
    if x > 0 {}
    if x > 1 {}
    if x > 2 {}
    if x > 3 {}
    if x > 4 {}
    if x > 5 {}
    if x > 6 {}
    if x > 7 {}
    if x > 8 {}
    if x > 9 {}
    if x > 10 {}
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("12"));
    }

    #[test]
    fn counts_logical_operators() {
        let src = r#"
fn check(a: bool, b: bool, c: bool, d: bool, e: bool) -> bool {
    if a && b && c && d && e {
        return true;
    }
    false
}
"#;
        assert!(run_on(src).is_empty());
    }
}
