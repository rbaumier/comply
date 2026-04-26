//! node-callback-return backend — require return after callback calls.

use crate::diagnostic::{Diagnostic, Severity};

const CALLBACKS: &[&str] = &["callback", "cb", "next"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
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

    // `cb(err);` as an expression_statement — check if it's the last statement in a function body.
    if pk == "expression_statement"
        && let Some(block) = parent.parent()
            && block.kind() == "statement_block" {
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
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-callback-return".into(),
        message: "Expected `return` with your callback function.".into(),
        severity: Severity::Warning,
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
}
