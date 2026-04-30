//! prefer-while Rust backend.
//!
//! Flag `loop { if !cond { break; } ... }` that should be `while cond { ... }`.
//! In Rust there's no `for(;;)` — the equivalent is `loop {}`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["loop_expression"] => |node, source, ctx, diagnostics|
    // Get the body block.
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "block" {
        return;
    }

    // Check if the first named child is an if_expression with a break.
    let Some(first_stmt) = body.named_child(0) else { return };
    // Unwrap expression_statement wrapper if present.
    let if_node = if first_stmt.kind() == "expression_statement" {
        match first_stmt.named_child(0) {
            Some(c) if c.kind() == "if_expression" => c,
            _ => return,
        }
    } else if first_stmt.kind() == "if_expression" {
        first_stmt
    } else {
        return;
    };

    if if_node.child_by_field_name("alternative").is_some() {
        return;
    }

    // Check that the if body contains only a break_expression.
    let Some(consequence) = if_node.child_by_field_name("consequence") else { return };
    if consequence.kind() != "block" {
        return;
    }
    if consequence.named_child_count() != 1 {
        return;
    }
    let Some(only_child) = consequence.named_child(0) else { return };
    // Unwrap expression_statement if present.
    let break_node = if only_child.kind() == "expression_statement" {
        match only_child.named_child(0) {
            Some(c) => c,
            None => return,
        }
    } else {
        only_child
    };
    if break_node.kind() != "break_expression" {
        return;
    }
    let Ok(break_text) = break_node.utf8_text(source) else { return };
    if break_text.trim() != "break" {
        return;
    }

    // The condition should start with `!` (negated) to be convertible to while.
    let Some(condition) = if_node.child_by_field_name("condition") else { return };
    let Ok(cond_text) = condition.utf8_text(source) else { return };
    if !cond_text.trim().starts_with('!') {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-while".into(),
        message: "Use `while` instead of `loop` with a break guard at the top.".into(),
        severity: Severity::Warning,
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
    fn flags_loop_with_break_guard() {
        let src = r#"
fn f() {
    loop {
        if !condition {
            break;
        }
        do_work();
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_plain_loop() {
        let src = r#"
fn f() {
    loop {
        do_work();
        if done {
            break;
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_while_loop() {
        let src = r#"
fn f() {
    while condition {
        do_work();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_labeled_break_guard() {
        let src = r#"
fn f() {
    'outer: loop {
        loop {
            if !condition {
                break 'outer;
            }
            do_work();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_value_break_guard() {
        let src = r#"
fn f() -> i32 {
    loop {
        if !condition {
            break 42;
        }
        do_work();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_break_guard_with_else() {
        let src = r#"
fn f() {
    loop {
        if !condition {
            break;
        } else {
            prepare();
        }
        do_work();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
