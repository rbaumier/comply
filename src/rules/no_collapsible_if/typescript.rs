//! no-collapsible-if — flag `if (a) { if (b) {} }` that should be
//! `if (a && b) {}`.
//!
//! Matches an `if_statement` whose body (`consequence`) is a
//! `statement_block` containing exactly one named child that is
//! another `if_statement` — and the outer if has no `else` clause.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_statement"] => |node, source, ctx, diagnostics|
    // The outer if must NOT have an else clause
    if node.child_by_field_name("alternative").is_some() {
        return;
    }

    // Get the consequence (body) of the outer if
    let Some(body) = node.child_by_field_name("consequence") else { return };

    if body.kind() != "statement_block" {
        return;
    }

    // The body must contain exactly one named child
    if body.named_child_count() != 1 {
        return;
    }

    let Some(only_child) = body.named_child(0) else { return };

    // That single child must be an if_statement
    if only_child.kind() != "if_statement" {
        return;
    }

    // The inner if must also NOT have an else clause to be collapsible
    if only_child.child_by_field_name("alternative").is_some() {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-collapsible-if".into(),
        message: "Nested `if` without `else` can be merged into a single `if (a && b)`.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_nested_if() {
        let src = r#"
if (a) {
  if (b) {
    doSomething();
  }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_if_else_if() {
        let src = r#"
if (a) {
  doSomething();
} else if (b) {
  doOther();
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_outer_if_with_else() {
        // Outer if has an else — not collapsible
        let src = r#"
if (a) {
  if (b) {
    doSomething();
  }
} else {
  doOther();
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_inner_if_with_else() {
        // Inner if has an else — not collapsible with &&
        let src = r#"
if (a) {
  if (b) {
    doSomething();
  } else {
    doOther();
  }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_multiple_statements_in_body() {
        let src = r#"
if (a) {
  setup();
  if (b) {
    doSomething();
  }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_if() {
        assert!(run_on("if (a) { doSomething(); }").is_empty());
    }
}
