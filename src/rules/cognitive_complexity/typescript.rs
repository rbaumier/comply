//! cognitive-complexity AST backend — flag functions whose cognitive
//! complexity exceeds a configurable threshold (default 5).

use crate::diagnostic::{Diagnostic, Severity};

// Per SonarSource Cognitive Complexity: only the `switch` STATEMENT adds +1
// — individual `case` clauses are continuations and don't count on their own.
// Nesting still applies to whatever lives inside a case body.
const FLOW_KINDS: &[&str] = &[
    "if_statement",
    "else_clause",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "switch_statement",
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

/// Test-only: parse a TS source snippet, locate the first function-like
/// declaration, and return its body's cognitive complexity as computed by
/// this backend. Used by the shared-scenarios tests in `mod.rs`.
#[cfg(test)]
pub(super) fn compute_source(source: &str) -> u32 {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
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
    if is_function_node(node.kind()) {
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
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_when_complexity_exceeds_threshold() {
        // Symmetric to the Rust backend test: nested if/for/if/if/switch.
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
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("11"), "got: {}", d[0].message);
    }

    #[test]
    fn clean_function_below_threshold_is_not_flagged() {
        let src = "function add(a, b) { return a + b; }";
        assert!(run_on(src).is_empty());
    }
}
