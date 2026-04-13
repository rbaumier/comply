//! justify-inaction TypeScript / JavaScript / TSX backend.
//!
//! Flags empty control-flow blocks that have no comment inside
//! explaining why. Targets:
//!
//! - `catch_clause.body` — `try {} catch (e) {}` silent swallow.
//! - `finally_clause.body` — pointless finally.
//! - `if_statement.consequence` — `if (x) {}`.
//! - `else_clause`'s inner `statement_block` — `else {}`.
//! - `switch_default` — bare `default:` or `default: {}` catch-all.
//! - `while_statement.body` / `do_statement.body` /
//!   `for_statement.body` / `for_in_statement.body` /
//!   `for_of_statement.body` — empty loop body.
//!
//! A `statement_block` is justified (not flagged) if it contains at
//! least one `comment` named child. Function / method / arrow / class
//! method bodies are out of scope — they are commonly stubbed.

use crate::diagnostic::{Diagnostic, Severity};

fn stmt_block_is_empty(node: tree_sitter::Node) -> bool {
    node.kind() == "statement_block" && node.named_child_count() == 0
}

fn loop_name(kind: &str) -> &'static str {
    match kind {
        "while_statement" => "while",
        "do_statement" => "do-while",
        "for_statement" => "for",
        "for_in_statement" => "for-in",
        "for_of_statement" => "for-of",
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
    if !stmt_block_is_empty(body) {
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
    });
}

crate::ast_check! { |node, _source, ctx, diagnostics|
    match node.kind() {
        "catch_clause" => {
            if let Some(body) = node.child_by_field_name("body") {
                flag_empty(node, body, "catch", ctx, diagnostics);
            }
        }
        "finally_clause" => {
            if let Some(body) = node.child_by_field_name("body") {
                flag_empty(node, body, "finally", ctx, diagnostics);
            }
        }
        "if_statement" => {
            if let Some(cons) = node.child_by_field_name("consequence") {
                flag_empty(node, cons, "if", ctx, diagnostics);
            }
        }
        "else_clause" => {
            // `else_clause` wraps either a `statement_block` (plain else)
            // or an `if_statement` (else-if). Only plain else is ours.
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "statement_block" {
                    flag_empty(node, child, "else", ctx, diagnostics);
                    break;
                }
            }
        }
        "while_statement" | "do_statement" | "for_statement"
        | "for_in_statement" | "for_of_statement" => {
            if let Some(body) = node.child_by_field_name("body") {
                flag_empty(node, body, loop_name(node.kind()), ctx, diagnostics);
            }
        }
        "switch_default" => {
            // Two shapes in tree-sitter-typescript:
            //   `default: { }`  — `body` field points at a `statement_block`.
            //   `default:`      — no body field; direct children are statements.
            let body_empty = match node.child_by_field_name("body") {
                Some(b) if b.kind() == "statement_block" => {
                    b.named_child_count() == 0
                }
                _ => node.named_child_count() == 0,
            };
            if body_empty {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "justify-inaction".into(),
                    message: "Empty `default` case \u{2014} add a comment inside explaining why the inaction is intentional.".into(),
                    severity: Severity::Warning,
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    // ── catch / finally ──────────────────────────────────────────

    #[test]
    fn flags_empty_catch() {
        assert_eq!(run_on("try { x(); } catch (e) {}").len(), 1);
    }

    #[test]
    fn allows_catch_with_comment_inside() {
        assert!(
            run_on("try { x(); } catch (e) { /* swallowed intentionally */ }").is_empty()
        );
    }

    #[test]
    fn flags_empty_finally() {
        assert_eq!(run_on("try { x(); } finally {}").len(), 1);
    }

    // ── if / else ────────────────────────────────────────────────

    #[test]
    fn flags_empty_if() {
        assert_eq!(run_on("if (x) {}").len(), 1);
    }

    #[test]
    fn flags_empty_else() {
        assert_eq!(run_on("if (x) { a(); } else {}").len(), 1);
    }

    #[test]
    fn allows_else_with_comment_inside() {
        let src = "if (x) { a(); } else { /* no-op by design */ }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_else_if_chain() {
        assert!(run_on("if (x === 1) { a(); } else if (x === 2) { b(); }").is_empty());
    }

    // ── switch default ───────────────────────────────────────────

    #[test]
    fn flags_empty_default_block() {
        let src = "switch (x) { case 1: a(); break; default: {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_default_block_with_comment() {
        let src = "switch (x) { case 1: a(); break; default: { /* handled above */ } }";
        assert!(run_on(src).is_empty());
    }

    // ── loops ────────────────────────────────────────────────────

    #[test]
    fn flags_empty_while() {
        assert_eq!(run_on("while (poll()) {}").len(), 1);
    }

    #[test]
    fn flags_empty_do_while() {
        assert_eq!(run_on("do {} while (cond());").len(), 1);
    }

    #[test]
    fn flags_empty_for() {
        assert_eq!(run_on("for (let i = 0; i < 10; i++) {}").len(), 1);
    }

    #[test]
    fn flags_empty_for_of() {
        assert_eq!(run_on("for (const x of xs) {}").len(), 1);
    }

    #[test]
    fn allows_busy_wait_with_comment() {
        let src = "while (poll()) { /* busy wait for the device */ }";
        assert!(run_on(src).is_empty());
    }

    // ── scope exclusions ─────────────────────────────────────────

    #[test]
    fn does_not_flag_empty_function_body() {
        assert!(run_on("function stub() {}").is_empty());
    }

    #[test]
    fn does_not_flag_empty_arrow_body() {
        assert!(run_on("const noop = () => {};").is_empty());
    }

    #[test]
    fn does_not_flag_empty_method_body() {
        assert!(run_on("class Foo { bar() {} }").is_empty());
    }
}
