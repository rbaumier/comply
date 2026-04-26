//! justify-inaction Rust backend.
//!
//! Flags empty control-flow blocks that have no comment inside
//! explaining why. Targets:
//!
//! - `if_expression.consequence` — empty `if cond { }`.
//! - `else_clause`'s `block` child — empty `else { }`.
//! - `match_arm.value` when the value is an empty `block` — the
//!   canonical "silent ignore" pattern `None => {}` / `Err(_) => {}` /
//!   `_ => {}`.
//! - `for_expression.body` / `while_expression.body` /
//!   `loop_expression.body` — empty loop body.
//!
//! A block is considered justified and NOT flagged if it contains at
//! least one comment child (`line_comment` / `block_comment`). Any
//! other named child also makes the block non-empty by definition.
//!
//! Function bodies, closure bodies, and empty `{}` used as unit
//! expressions in other positions are out of scope — they are common
//! in stubs, marker impls, and no-op callbacks, and flagging them
//! would be pure noise.

use crate::diagnostic::{Diagnostic, Severity};

fn block_is_empty(node: tree_sitter::Node) -> bool {
    node.kind() == "block" && node.named_child_count() == 0
}

fn loop_name(kind: &str) -> &'static str {
    match kind {
        "for_expression" => "for",
        "while_expression" => "while",
        "loop_expression" => "loop",
        _ => "loop",
    }
}

fn flag_empty(
    container: tree_sitter::Node,
    body: tree_sitter::Node,
    what: &str,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !block_is_empty(body) {
        return;
    }
    let pos = container.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "justify-inaction".into(),
        message: format!(
            "Empty `{what}` block \u{2014} add a comment inside explaining why the inaction is intentional."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

crate::ast_check! { on ["if_expression", "else_clause", "match_arm", "for_expression", "while_expression", "loop_expression"] => |node, _source, ctx, diagnostics|
match node.kind() {
        "if_expression" => {
            if let Some(cons) = node.child_by_field_name("consequence") {
                flag_empty(node, cons, "if", ctx, diagnostics);
            }
        }
        "else_clause" => {
            // `else_clause` either wraps a `block` (plain else) or an
            // `if_expression` (else-if). We only care about plain else.
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "block" {
                    flag_empty(node, child, "else", ctx, diagnostics);
                    break;
                }
            }
        }
        "match_arm" => {
            if let Some(value) = node.child_by_field_name("value") {
                flag_empty(node, value, "match arm", ctx, diagnostics);
            }
        }
        "for_expression" | "while_expression" | "loop_expression" => {
            if let Some(body) = node.child_by_field_name("body") {
                flag_empty(node, body, loop_name(node.kind()), ctx, diagnostics);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    // ── if / else ────────────────────────────────────────────────

    #[test]
    fn flags_empty_if() {
        assert_eq!(run_on("fn f(x: bool) { if x {} }").len(), 1);
    }

    #[test]
    fn flags_empty_else() {
        let src = "fn f(x: bool) { if x { go(); } else {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_if_with_comment_inside() {
        let src = "fn f(x: bool) { if x { /* handled upstream */ } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_else_with_line_comment_inside() {
        let src = "fn f(x: bool) {\n    if x {\n        go();\n    } else {\n        // intentional no-op\n    }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_empty_else() {
        let src = "fn f(x: bool) { if x { a(); } else { b(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_else_if_chain() {
        // `else if` wraps an if_expression, not an empty block.
        let src = "fn f(x: i32) { if x == 1 { a(); } else if x == 2 { b(); } }";
        assert!(run_on(src).is_empty());
    }

    // ── match arms ───────────────────────────────────────────────

    #[test]
    fn flags_empty_none_arm() {
        let src = "fn f(x: Option<u8>) { match x { Some(v) => go(v), None => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_err_arm() {
        let src = "fn f(r: Result<u8, E>) { match r { Ok(v) => go(v), Err(_) => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_empty_wildcard_arm() {
        let src = "fn f(x: u8) { match x { 0 => go(), _ => {} } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_match_arm_with_comment_inside() {
        let src = r#"
fn f(x: Option<u8>) {
    match x {
        Some(v) => go(v),
        None => {
            // already handled upstream
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_empty_match_arm() {
        let src = "fn f(x: u8) { match x { 0 => {}, _ => go() } }";
        // First arm is empty → flagged. Second has a call.
        assert_eq!(run_on(src).len(), 1);
    }

    // ── loops ────────────────────────────────────────────────────

    #[test]
    fn flags_empty_while() {
        assert_eq!(run_on("fn f() { while poll() {} }").len(), 1);
    }

    #[test]
    fn flags_empty_for() {
        assert_eq!(run_on("fn f(xs: &[u8]) { for _ in xs {} }").len(), 1);
    }

    #[test]
    fn flags_empty_loop() {
        assert_eq!(run_on("fn f() { loop {} }").len(), 1);
    }

    #[test]
    fn allows_while_with_comment() {
        let src = "fn f() { while poll() { /* busy-wait for the device */ } }";
        assert!(run_on(src).is_empty());
    }

    // ── scope exclusions ─────────────────────────────────────────

    #[test]
    fn does_not_flag_empty_fn_body() {
        // Marker / stub fn — out of scope.
        assert!(run_on("fn stub() {}").is_empty());
    }

    #[test]
    fn does_not_flag_empty_closure_body() {
        let src = "fn f() { let cb = || {}; cb(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_unit_match_arm() {
        // `None => ()` is a unit expression, not a block — out of scope.
        let src = "fn f(x: Option<u8>) { match x { Some(v) => go(v), None => () } }";
        assert!(run_on(src).is_empty());
    }
}
