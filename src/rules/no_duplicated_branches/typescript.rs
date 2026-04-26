//! no-duplicated-branches — flag if/else branches with identical bodies.
//!
//! Matches `if_statement` nodes, collects the text of the consequence
//! and all else/else-if branches, and flags duplicates.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_statement"] => |node, source, ctx, diagnostics|
    // Only process the outermost if in a chain (skip if parent is else_clause)
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
        }

    // Collect all branch bodies in the if/else-if/else chain
    let mut bodies: Vec<(usize, String)> = Vec::new();
    collect_branch_bodies(node, source, &mut bodies);

    if bodies.len() < 2 {
        return;
    }

    // Report each duplicate line once: for each j >= 1, if bodies[j]
    // matches any earlier bodies[i], emit exactly one diagnostic at line j.
    let mut reported: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for j in 1..bodies.len() {
        if bodies[j].1.is_empty() {
            continue;
        }
        for i in 0..j {
            if bodies[i].1.is_empty() {
                continue;
            }
            if bodies[i].1 == bodies[j].1 && reported.insert(bodies[j].0) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: bodies[j].0,
                    column: 1,
                    rule_id: "no-duplicated-branches".into(),
                    message: "This branch has the same body as another branch — merge conditions or remove the duplicate.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
    }
}

/// Recursively collect branch bodies from an if/else-if/else chain.
fn collect_branch_bodies(
    node: tree_sitter::Node,
    source: &[u8],
    bodies: &mut Vec<(usize, String)>,
) {
    // Get the consequence (body) of this if
    if let Some(body) = node.child_by_field_name("consequence") {
        let line = body.start_position().row + 1;
        let text = body_text(&body, source);
        bodies.push((line, text));
    }

    // Check for alternative (else/else-if)
    if let Some(alt) = node.child_by_field_name("alternative") {
        // alt is an `else_clause`
        let mut cursor = alt.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "if_statement" => {
                        // else if — recurse
                        collect_branch_bodies(child, source, bodies);
                        return;
                    }
                    "statement_block" => {
                        // plain else
                        let line = child.start_position().row + 1;
                        let text = body_text(&child, source);
                        bodies.push((line, text));
                        return;
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}

/// Extract normalized body text from a statement_block, stripping the
/// outer braces and normalizing whitespace for comparison.
fn body_text(node: &tree_sitter::Node, source: &[u8]) -> String {
    let mut parts = Vec::new();
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i)
            && let Ok(t) = child.utf8_text(source) {
                parts.push(t.trim().to_string());
            }
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_if_else() {
        let src = "\
if (a) {
  doSomething();
} else {
  doSomething();
}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_duplicate_in_else_if_chain() {
        let src = "\
if (a) {
  foo();
} else if (b) {
  bar();
} else if (c) {
  foo();
}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_branches() {
        let src = "\
if (a) {
  foo();
} else {
  bar();
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_branch() {
        let src = "\
if (a) {
  foo();
}";
        assert!(run_on(src).is_empty());
    }

    /// Three branches with the same body should emit 2 diagnostics (one
    /// per duplicate line), not 3 — the previous pairwise loop reported
    /// line `j` once per earlier match.
    #[test]
    fn dedups_three_identical_branches() {
        let src = "\
if (a) {
  foo();
} else if (b) {
  foo();
} else {
  foo();
}";
        assert_eq!(run_on(src).len(), 2);
    }
}
