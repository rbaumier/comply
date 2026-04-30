//! db-no-n-plus-one Rust backend.
//!
//! Flag `.await` on DB-like calls inside loops. In Rust this looks like
//! `query(...).await` inside `for`/`while`/`loop` blocks.

use crate::diagnostic::{Diagnostic, Severity};

const QUERY_METHODS: &[&str] = &[
    "query",
    "execute",
    "fetch_one",
    "fetch_all",
    "fetch_optional",
    "find",
    "insert",
    "update",
    "delete",
];

fn is_db_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = node.utf8_text(source).unwrap_or("");
    QUERY_METHODS
        .iter()
        .any(|m| text.contains(&format!(".{m}(")))
}

fn is_inside_loop(node: tree_sitter::Node) -> bool {
    let mut parent = node.parent();
    while let Some(p) = parent {
        match p.kind() {
            "for_expression" | "while_expression" | "loop_expression" => return true,
            "function_item" | "closure_expression" => return false,
            _ => {}
        }
        parent = p.parent();
    }
    false
}

crate::ast_check! { on ["await_expression"] => |node, source, ctx, diagnostics|
    if !is_inside_loop(node) {
        return;
    }

    let Some(inner) = node.named_child(0) else { return };
    if !is_db_call(inner, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "db-no-n-plus-one".into(),
        message: "Awaited DB query inside a loop — use a batch query or JOIN.".into(),
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
    fn flags_query_in_loop() {
        let src = "async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_query_outside_loop() {
        let src = "async fn f() { db.query(1).await; }";
        assert!(run_on(src).is_empty());
    }
}
