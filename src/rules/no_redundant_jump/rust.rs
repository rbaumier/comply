//! no-redundant-jump Rust backend.
//!
//! `return;` (bare) is redundant iff walking up from it reaches a
//! `function_item` / `closure_expression` through tail positions only.
//! `continue;` is redundant iff walking up reaches a loop boundary
//! (`for_expression` / `while_expression` / `loop_expression`).
//!
//! "Tail position" means: when the current node's parent is a `block`,
//! the node must be the last named child of that block; when it is an
//! `if_expression` / `else_clause` / `match_arm` / `match_block` /
//! `match_expression`, walking through is always valid because every
//! branch of an if/match is a parallel tail. Any other parent kind
//! (a `let_declaration`, a macro argument, …) is treated conservatively
//! as not-in-tail-position and yields NOT_REDUNDANT.

use crate::diagnostic::{Diagnostic, Severity};

#[derive(Copy, Clone, PartialEq, Eq)]
enum JumpKind {
    Return,
    Continue,
}

crate::ast_check! { on ["return_expression", "continue_expression"] => |node, _source, ctx, diagnostics|
    let kind = match node.kind() {
        "return_expression" => {
            if node.named_child_count() != 0 {
                return;
            }
            JumpKind::Return
        }
        "continue_expression" => JumpKind::Continue,
        _ => return,
    };

    if !is_redundant(node, kind) {
        return;
    }

    let pos = node.start_position();
    let keyword = match kind {
        JumpKind::Return => "return;",
        JumpKind::Continue => "continue;",
    };
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-redundant-jump".into(),
        message: format!(
            "Redundant `{keyword}` \u{2014} execution already falls through here."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

fn is_redundant(start: tree_sitter::Node, kind: JumpKind) -> bool {
    let mut node = start;
    loop {
        let Some(parent) = node.parent() else {
            return false;
        };
        match parent.kind() {
            "function_item" | "closure_expression" => {
                return kind == JumpKind::Return;
            }
            "for_expression" | "while_expression" | "loop_expression" => {
                return kind == JumpKind::Continue;
            }
            "block" => {
                if !is_last_named_child(parent, node) {
                    return false;
                }
                node = parent;
            }
            "expression_statement"
            | "if_expression"
            | "else_clause"
            | "match_arm"
            | "match_block"
            | "match_expression" => {
                node = parent;
            }
            _ => return false,
        }
    }
}

fn is_last_named_child(parent: tree_sitter::Node, child: tree_sitter::Node) -> bool {
    let count = parent.named_child_count();
    if count == 0 {
        return false;
    }
    parent.named_child(count - 1).map(|n| n.id()) == Some(child.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_redundant_return_at_fn_end() {
        let src = "fn foo() {\n    do_stuff();\n    return;\n}";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return;"));
    }

    #[test]
    fn flags_redundant_continue_at_loop_end() {
        let src = r#"
fn f(xs: &[i32]) {
    for x in xs {
        do_stuff(x);
        continue;
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("continue;"));
    }

    #[test]
    fn allows_return_before_more_code() {
        let src = "fn foo(x: bool) {\n    if x {\n        return;\n    }\n    bar();\n}";
        assert!(run_on(src).is_empty());
    }

    /// FP observed on react_no_object_type_as_default_prop/typescript.rs:102.
    /// Nested `if` where the inner return is the early-exit guard of the
    /// outer `if`, followed by unrelated code in the function body.
    #[test]
    fn allows_nested_if_guard_with_more_fn_body() {
        let src = r#"
fn check(is_arrow: bool, node: u32, source: &str) {
    if is_arrow {
        let parent = node + 1;
        if parent == 0 {
            return;
        }
        if parent != 42 {
            return;
        }
        let name = parent + 1;
        let t = source;
        if !t.starts_with(|c: char| c.is_ascii_uppercase()) {
            return;
        }
    }

    let mut stack: Vec<u32> = vec![node];
    while let Some(_current) = stack.pop() {
        do_stuff();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_return_at_end_of_else() {
        let src = r#"
fn f(a: bool) {
    if a {
        x();
    } else {
        y();
        return;
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_return_with_value() {
        let src = r#"
fn f() -> i32 {
    do_stuff();
    return 42;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_continue_in_nested_if_last_of_loop() {
        let src = r#"
fn f(xs: &[i32]) {
    for x in xs {
        do_stuff(*x);
        if *x == 0 {
            continue;
        }
    }
}
"#;
        // The inner `continue;` is the last thing in the loop body's
        // block via the if, walking up: block(if) → if_expression →
        // expression_statement → block(for body) → for_expression.
        // The if is last-child of for body → REDUNDANT.
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_continue_before_more_loop_body() {
        let src = r#"
fn f(xs: &[i32]) {
    for x in xs {
        if *x < 0 {
            continue;
        }
        do_stuff(*x);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_return_in_match_arm_at_fn_end() {
        let src = r#"
fn f(x: u8) {
    match x {
        0 => {
            do_stuff();
            return;
        }
        _ => {}
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_return_in_match_arm_before_more_code() {
        let src = r#"
fn f(x: u8) {
    match x {
        0 => {
            do_stuff();
            return;
        }
        _ => {}
    }
    bar();
}
"#;
        assert!(run_on(src).is_empty());
    }
}
