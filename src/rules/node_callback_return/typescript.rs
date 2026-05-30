//! node-callback-return backend — require return after callback calls.

use crate::diagnostic::{Diagnostic, Severity};

const CALLBACKS: &[&str] = &["callback", "cb", "next"];

crate::ast_check! { on ["call_expression"] prefilter = ["callback", "cb", "next"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");

    if !CALLBACKS.contains(&callee_text) {
        return;
    }

    // Walk up: if the call is in a return statement or is the body of an arrow function, it's fine.
    let Some(parent) = node.parent() else { return };
    let pk = parent.kind();

    // `return cb(err);` — expression inside return_statement
    if pk == "return_statement" {
        return;
    }

    // Arrow function body: `(err) => cb(err)` (expression, not block)
    if pk == "arrow_function" {
        return;
    }

    // `const result = await callback(...)` or `return await callback(...)` —
    // result captured for post-callback cleanup, or explicitly returned.
    // Neither is the Node error-first callback pattern.
    if pk == "await_expression" {
        if let Some(await_parent) = parent.parent() {
            let apk = await_parent.kind();
            if apk == "variable_declarator" || apk == "return_statement" {
                return;
            }
        }
    }

    // `cb(err);` as an expression_statement — check if it's followed by control flow or is the last statement in a function body.
    if pk == "expression_statement"
        && let Some(block) = parent.parent()
            && block.kind() == "statement_block" {
                if next_statement_exits(block, parent) {
                    return;
                }

                // Check if this expression_statement is the last child in the block.
                let mut cursor = block.walk();
                let last_stmt = block.children(&mut cursor)
                    .filter(|c| c.kind() != "{" && c.kind() != "}" && c.kind() != "comment")
                    .last();
                if let Some(last) = last_stmt
                    && last.id() == parent.id() {
                        // Last statement in block — check if block belongs to a function.
                        if let Some(func) = block.parent() {
                            let fk = func.kind();
                            if fk == "function_declaration" || fk == "function" || fk == "arrow_function" || fk == "method_definition" {
                                return;
                            }
                        }
                    }
            }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-callback-return".into(),
        message: "Expected `return` with your callback function.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn next_statement_exits(block: tree_sitter::Node, statement: tree_sitter::Node) -> bool {
    let mut cursor = block.walk();
    let mut found_current = false;
    for child in block.named_children(&mut cursor) {
        if found_current {
            return matches!(child.kind(), "return_statement" | "throw_statement");
        }
        if child.id() == statement.id() {
            found_current = true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_cb_without_return() {
        let src = "function handle(err) { if (err) { cb(err); } doMore(); }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return"));
    }

    #[test]
    fn allows_return_cb() {
        let src = "function handle(err) { if (err) { return cb(err); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_cb_as_last_in_function() {
        let src = "function handle(err) { cb(err); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_cb_followed_by_return() {
        let src = "function handle(err) { if (err) { cb(err); return; } doMore(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_cb_followed_by_throw() {
        let src = "function handle(err) { if (err) { cb(err); throw err; } doMore(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_cb_followed_by_more_work() {
        let src = "function handle(err) { if (err) { cb(err); doMore(); } finish(); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_when_callback_result_is_awaited_and_captured() {
        // Regression #547: `const result = await callback(conn)` followed by cleanup
        // before `return result` must not be flagged.
        let src = r#"
            async function wrap(callback) {
              try {
                const result = await callback(conn);
                await conn.unsafe("RELEASE SAVEPOINT sp");
                return result;
              } catch (err) {
                await conn.unsafe("ROLLBACK TO SAVEPOINT sp");
                throw err;
              }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_return_await_callback() {
        // `return await callback(...)` — explicitly returned, not a Node FP.
        let src = "async function wrap(callback) { return await callback(conn); }";
        assert!(run_on(src).is_empty());
    }
}
