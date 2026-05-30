//! cognitive-complexity Rust backend.
//!
//! Same concept as the TS backend: count flow structures, logical operators,
//! and nesting depth inside each `function_item`.

use crate::diagnostic::{Diagnostic, Severity};

// Per SonarSource Cognitive Complexity: only the match/switch EXPRESSION adds
// +1 — the individual arms/cases are continuations and add nothing on their
// own. The match does NOT increase the nesting level for its arm bodies: exhaustive
// discriminated-union matches enumerate unavoidable paths and should not penalise
// nested logic inside each arm with an extra nesting increment.
const FLOW_KINDS: &[&str] = &[
    "if_expression",
    "else_clause",
    "for_expression",
    "while_expression",
    "loop_expression",
    "match_expression",
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
    // match_expression is intentionally excluded: its arms enumerate
    // predetermined paths and must not penalise code inside them with an
    // extra nesting level.
    let nest_increase = matches!(
        kind,
        "if_expression" | "for_expression" | "while_expression" | "loop_expression"
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

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "block" {
        return;
    }

    let threshold = ctx.config.threshold("cognitive-complexity", "max", ctx.lang) as u32;
    let complexity = compute(body, source, 0);

    if complexity > threshold {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "cognitive-complexity".into(),
            message: format!(
                "Cognitive complexity is {complexity} (threshold {threshold}). Simplify this function."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Test-only: parse a Rust source snippet, locate the first `fn`, and
/// return its body's cognitive complexity as computed by this backend.
/// Used by the shared-scenarios tests in `mod.rs` to assert exact scores
/// across both the Rust and TypeScript backends.
#[cfg(test)]
pub(super) fn compute_source(source: &str) -> u32 {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&lang).expect("grammar should load");
    let tree = parser
        .parse(source, None)
        .expect("parser should produce a tree");
    find_first_fn_body(tree.root_node())
        .map(|body| compute(body, source.as_bytes(), 0))
        .unwrap_or(0)
}

#[cfg(test)]
fn find_first_fn_body(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() == "function_item" {
        return node.child_by_field_name("body");
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(body) = find_first_fn_body(child) {
            return Some(body);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_when_complexity_exceeds_threshold() {
        // Deeply nested if/for structure (no switch/match):
        // if        +1 (nesting 0)
        // for       +1 (nesting 0) → nesting 1
        //   if      +2 (nesting 1) → nesting 2
        //     if    +3 (nesting 2) → nesting 3
        //       if  +4 (nesting 3) → nesting 4
        //       for +5 (nesting 4) → nesting 5
        //         if  +6 (nesting 5) → nesting 6
        //           if  +7 (nesting 6) → nesting 7
        //             if  +8 (nesting 7)
        // Total: 1+1+2+3+4+5+6+7+8 = 37 (threshold 30)
        let src = r#"fn process(items: &[i32]) {
    if items.is_empty() {
        return;
    }
    for item in items {
        if *item > 0 {
            if *item > 5 {
                if *item > 10 {
                    for sub in 0..*item {
                        if sub > 0 {
                            if sub > 2 {
                                if sub > 3 {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("37"), "got: {}", d[0].message);
    }

    #[test]
    fn clean_function_below_threshold_is_not_flagged() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }";
        assert!(run_on(src).is_empty());
    }
}
