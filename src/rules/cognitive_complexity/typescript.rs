//! cognitive-complexity AST backend — flag functions whose cognitive
//! complexity exceeds a configurable threshold (default 5).

use crate::diagnostic::{Diagnostic, Severity};

// Per SonarSource Cognitive Complexity: only the `switch` STATEMENT adds +1
// — individual `case` clauses are continuations and don't count on their own.
// The switch does NOT increase the nesting level for its case bodies: exhaustive
// discriminated-union switches enumerate unavoidable paths and should not penalise
// nested logic inside each arm with an extra nesting increment.
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
    // switch_statement is intentionally excluded: its case arms enumerate
    // predetermined paths and must not penalise code inside them with an
    // extra nesting level.
    let nest_increase = matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "for_in_statement"
            | "while_statement"
            | "do_statement"
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
        // Deeply nested if/for structure (no switch):
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
        let src = r#"function process(items) {
  if (items.length === 0) {
    return;
  }
  for (const item of items) {
    if (item.active) {
      if (item.value > 5) {
        if (item.value > 10) {
          for (const sub of item.subs) {
            if (sub.ok) {
              if (sub.valid) {
                if (sub.ready) {
                  return sub.result;
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
        let src = "function add(a, b) { return a + b; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_exhaustive_switch_with_per_case_logic() {
        // Regression for #586: exhaustive error-map switches with conditional logic
        // per case must not trigger cognitive-complexity. The switch no longer
        // increases the nesting level for its case bodies.
        let src = r#"function zodErrorMap(issue) {
  switch (issue.code) {
    case 'invalid_type':
      if (issue.received === 'undefined') return 'Required';
      return 'Invalid type';
    case 'too_small':
      if (issue.type === 'array') return 'Too few items';
      if (issue.type === 'string') return 'Too short';
      return 'Value too small';
    case 'too_big':
      if (issue.type === 'string') return 'Too long';
      return 'Value too large';
    case 'invalid_string':
      if (issue.validation === 'email') return 'Invalid email';
      if (issue.validation === 'url') return 'Invalid URL';
      return 'Invalid string';
    case 'invalid_enum_value':
      return 'Invalid option';
    case 'invalid_literal':
      return 'Wrong value';
    case 'custom':
      return issue.params?.message ?? 'Invalid value';
    default:
      return 'Invalid value';
  }
}"#;
        assert!(run_on(src).is_empty());
    }
}
