//! no-identical-conditions backend — flag duplicate conditions in
//! if/else-if chains.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "if_statement" {
        return;
    }

    // Only process the top-level if (not nested else-if branches).
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
        }

    // Collect all conditions in this if/else-if chain.
    let mut conditions: Vec<(String, tree_sitter::Node)> = Vec::new();
    let mut current = Some(node);

    while let Some(if_node) = current {
        if if_node.kind() != "if_statement" {
            break;
        }

        if let Some(cond) = if_node.child_by_field_name("condition")
            && let Ok(cond_text) = cond.utf8_text(source) {
                // Check for duplicates.
                for (prev_text, _) in &conditions {
                    if prev_text == cond_text {
                        let pos = cond.start_position();
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "no-identical-conditions".into(),
                            message: format!(
                                "Duplicate condition `{}` in if/else-if chain.",
                                cond_text.trim_start_matches('(').trim_end_matches(')')
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        break;
                    }
                }
                conditions.push((cond_text.to_string(), cond));
            }

        // Follow the else clause to the next if_statement.
        current = None;
        if let Some(alt) = if_node.child_by_field_name("alternative") {
            // else_clause's first named child is the if_statement (else if)
            // or statement_block (else { ... }).
            let count = alt.named_child_count();
            for i in 0..count {
                if let Some(child) = alt.named_child(i)
                    && child.kind() == "if_statement" {
                        current = Some(child);
                        break;
                    }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_condition() {
        let src = "\
if (x > 0) {
  doA();
} else if (x > 0) {
  doB();
}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_different_conditions() {
        let src = "\
if (x > 0) {
  doA();
} else if (x < 0) {
  doB();
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_multiple_duplicates() {
        let src = "\
if (a === 1) {
  x();
} else if (b === 2) {
  y();
} else if (a === 1) {
  z();
} else if (b === 2) {
  w();
}";
        assert_eq!(run_on(src).len(), 2);
    }
}
