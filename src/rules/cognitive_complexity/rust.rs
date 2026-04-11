//! cognitive-complexity Rust backend.
//!
//! Same concept as the TS backend: count flow structures, logical operators,
//! and nesting depth inside each `function_item`.

use crate::diagnostic::{Diagnostic, Severity};

const FLOW_KINDS: &[&str] = &[
    "if_expression",
    "else_clause",
    "for_expression",
    "while_expression",
    "loop_expression",
    "match_arm",
];

const LOGICAL_OPS: &[&str] = &["&&", "||"];

/// Recursively compute cognitive complexity of a subtree.
fn compute(node: tree_sitter::Node, source: &[u8], nesting: u32) -> u32 {
    let mut score: u32 = 0;
    let kind = node.kind();

    let increments = FLOW_KINDS.contains(&kind);
    if increments {
        if kind == "else_clause" {
            // Check if the else contains a direct if_expression (else if).
            let has_direct_if = node
                .named_child(0)
                .is_some_and(|c| c.kind() == "if_expression");
            if !has_direct_if {
                score += 1;
            }
        } else {
            score += 1 + nesting;
        }
    }

    // Logical operators in binary expressions.
    if kind == "binary_expression"
        && let Some(op) = node.child_by_field_name("operator")
    {
        let op_text = op.utf8_text(source).unwrap_or("");
        if LOGICAL_OPS.contains(&op_text) {
            score += 1;
        }
    }

    // Nesting increases for blocks that are children of flow control.
    let nest_increase = matches!(
        kind,
        "if_expression"
            | "for_expression"
            | "while_expression"
            | "loop_expression"
            | "match_expression"
    );

    // Don't recurse into nested function definitions.
    let count = node.child_count();
    for i in 0..count {
        let child = node.child(i).unwrap();
        if matches!(child.kind(), "function_item" | "closure_expression") {
            continue;
        }
        let child_nesting = if nest_increase { nesting + 1 } else { nesting };
        score += compute(child, source, child_nesting);
    }

    score
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "function_item" {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "block" {
        return;
    }

    let threshold = ctx.config.threshold("cognitive-complexity", "max", 5) as u32;
    let complexity = compute(body, source, 0);

    if complexity > threshold {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "cognitive-complexity".into(),
            message: format!(
                "Cognitive complexity is {complexity} (threshold {threshold}). Simplify this function."
            ),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_complex_function() {
        let src = r#"fn process(items: &[Item]) {
    if items.is_empty() {
        return;
    }
    for item in items {
        if item.active {
            if item.value > 10 {
                match item.kind {
                    Kind::A => {},
                    Kind::B => {},
                }
            }
        }
    }
}"#;
        let d = run_on(src);
        assert!(!d.is_empty(), "should flag complex function");
    }

    #[test]
    fn allows_simple_function() {
        let src = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_moderate_function() {
        let src = r#"fn check(x: i32) -> bool {
    if x > 0 {
        return true;
    }
    false
}"#;
        assert!(run_on(src).is_empty());
    }
}
