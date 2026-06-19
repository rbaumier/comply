//! no-redundant-jump Rust backend.
//!
//! `return;` (bare) is redundant iff walking up from it reaches a
//! `function_item` / `closure_expression` through tail positions only.
//! `continue;` is redundant iff walking up reaches a loop boundary
//! (`for_expression` / `while_expression` / `loop_expression`).
//!
//! A jump carrying an operand (`return x`) or a label (`continue 'outer`)
//! is never flagged: a labeled `continue 'L` targets the loop named `'L`,
//! which may be an outer loop, so it is not redundant even in tail
//! position of the innermost loop.
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
        "continue_expression" => {
            if node.named_child_count() != 0 {
                return;
            }
            JumpKind::Continue
        }
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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

    /// Regression for #3897: a labeled `continue 'table` at the tail of an
    /// inner loop targets the OUTER labeled loop, so removing it changes the
    /// program. It must never be flagged.
    #[test]
    fn allows_labeled_continue_to_outer_loop() {
        let src = r#"
fn f(state: &[i32]) {
    'table: loop {
        for rule in state {
            do_x();
            continue 'table;
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Regression for #3897 (second syn shape): a labeled `continue 'outer`
    /// inside a nested bare `loop` targets a loop further out.
    #[test]
    fn allows_labeled_continue_from_nested_loop() {
        let src = r#"
fn f(b: u8, rest: i32) {
    let mut s = 0;
    'outer: loop {
        loop {
            match b {
                0 => s = rest,
                _ => continue 'outer,
            }
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// A bare `continue;` at the tail of a single loop body is still
    /// redundant — the labeled-continue guard must not suppress it.
    #[test]
    fn flags_bare_continue_at_tail_of_loop() {
        let src = r#"
fn f() {
    loop {
        do_x();
        continue;
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("continue;"));
    }
}
