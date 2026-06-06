//! no-collapsible-if Rust backend.
//!
//! Flag `if a { if b { } }` that should be `if a && b { }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_expression"] => |node, _source, ctx, diagnostics|
    // Skip else-if arms: merging them would harm readability of control-flow chains.
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
    }

    // The outer if must NOT have an else clause.
    if node.child_by_field_name("alternative").is_some() {
        return;
    }

    // Get the consequence (body) of the outer if.
    let Some(body) = node.child_by_field_name("consequence") else { return };

    if body.kind() != "block" {
        return;
    }

    // The body must contain exactly one named child.
    if body.named_child_count() != 1 {
        return;
    }

    let Some(only_child) = body.named_child(0) else { return };

    // In Rust tree-sitter, the if_expression is wrapped in expression_statement.
    let inner_if = if only_child.kind() == "expression_statement" {
        only_child.named_child(0)
    } else {
        Some(only_child)
    };
    let Some(inner_if) = inner_if else { return };

    // That single child must be an if_expression.
    if inner_if.kind() != "if_expression" {
        return;
    }

    // The inner if must also NOT have an else clause.
    if inner_if.child_by_field_name("alternative").is_some() {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-collapsible-if".into(),
        message: "Nested `if` without `else` can be merged into a single `if a && b`.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_nested_if() {
        let src = r#"
fn f() {
    if a {
        if b {
            do_something();
        }
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_outer_if_with_else() {
        let src = r#"
fn f() {
    if a {
        if b {
            do_something();
        }
    } else {
        do_other();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_inner_if_with_else() {
        let src = r#"
fn f() {
    if a {
        if b {
            do_something();
        } else {
            do_other();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_else_if_with_nested_if() {
        let src = r#"
fn f() {
    if a { do_a(); }
    else if b {
        if c {
            do_c();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_multiple_statements_in_body() {
        let src = r#"
fn f() {
    if a {
        setup();
        if b {
            do_something();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
