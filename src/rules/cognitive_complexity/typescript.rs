//! cognitive-complexity AST backend — flag functions whose cognitive
//! complexity exceeds a configurable threshold (default 5).

use crate::diagnostic::{Diagnostic, Severity};

const FLOW_KINDS: &[&str] = &[
    "if_statement",
    "else_clause",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "switch_case",
    "catch_clause",
    "ternary_expression",
];

const LOGICAL_OPS: &[&str] = &["&&", "||", "??"];

/// Recursively compute cognitive complexity of a subtree.
fn compute(node: tree_sitter::Node, source: &[u8], nesting: u32) -> u32 {
    let mut score: u32 = 0;
    let kind = node.kind();

    // Increment for flow structures.
    let increments = FLOW_KINDS.contains(&kind);
    if increments {
        // `else` without `if` (bare else) only adds 1, no nesting penalty.
        // `else if` counts as 1 (no nesting increment).
        if kind == "else_clause" {
            // Check if the else contains a direct if_statement (else if).
            let has_direct_if = node
                .named_child(0)
                .is_some_and(|c| c.kind() == "if_statement");
            if has_direct_if {
                // `else if` — count the inner if via recursion, don't add here.
            } else {
                score += 1;
            }
        } else {
            score += 1 + nesting;
        }
    }

    // Logical operators in binary expressions.
    if kind == "binary_expression"
        && let Some(op) = node.child_by_field_name("operator") {
            let op_text = op.utf8_text(source).unwrap_or("");
            if LOGICAL_OPS.contains(&op_text) {
                score += 1;
            }
        }

    // Nesting increases for blocks that are children of flow control.
    let nest_increase = matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "for_in_statement"
            | "while_statement"
            | "do_statement"
            | "switch_statement"
            | "catch_clause"
    );

    // Don't recurse into nested function definitions — they get their own score.
    let count = node.child_count();
    for i in 0..count {
        let child = node.child(i).unwrap();
        if matches!(
            child.kind(),
            "function_declaration"
                | "function"
                | "arrow_function"
                | "method_definition"
                | "generator_function"
                | "generator_function_declaration"
        ) {
            continue;
        }
        let child_nesting = if nest_increase { nesting + 1 } else { nesting };
        score += compute(child, source, child_nesting);
    }

    score
}

fn is_function_node(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration"
    )
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_function_node(node.kind()) {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        // Concise arrow — negligible complexity.
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
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_complex_function() {
        let src = r#"function process(items) {
  if (items.length === 0) {
    return;
  }
  for (const item of items) {
    if (item.active) {
      if (item.value > 10) {
        switch (item.type) {
          case 'a':
            break;
          case 'b':
            break;
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
        let src = "function add(a, b) {\n  return a + b;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_moderate_function() {
        let src = r#"function check(x) {
  if (x > 0) {
    return true;
  }
  return false;
}"#;
        assert!(run_on(src).is_empty());
    }
}
