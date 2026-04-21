use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const CLEANUP_METHODS: &[&str] =
    &["close", "dispose", "destroy", "disconnect", "release", "end"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "try_statement" {
                return;
            }
            let finally = match node.child_by_field_name("finalizer") {
                Some(f) => f,
                None => return,
            };
            // finally_clause wraps a statement_block — get the block
            let body = match finally.named_child(0) {
                Some(b) if b.kind() == "statement_block" => b,
                _ => return,
            };
            // Block must contain exactly one statement
            if body.named_child_count() != 1 {
                return;
            }
            let stmt = match body.named_child(0) {
                Some(s) => s,
                None => return,
            };
            if stmt.kind() != "expression_statement" {
                return;
            }
            let expr = match stmt.named_child(0) {
                Some(e) => e,
                None => return,
            };
            if expr.kind() != "call_expression" {
                return;
            }
            let func = match expr.child_by_field_name("function") {
                Some(f) => f,
                None => return,
            };
            if func.kind() != "member_expression" {
                return;
            }
            let prop = match func.child_by_field_name("property") {
                Some(p) => p,
                None => return,
            };
            let method = prop.utf8_text(source).unwrap_or("");
            if !CLEANUP_METHODS.iter().any(|&m| m == method) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Use `using` / `await using` instead of try/finally with `.{method}()` (TS 5.2+)."
                ),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_try_finally_close() {
        assert_eq!(run("const c = connect(); try { use(c) } finally { c.close() }").len(), 1);
    }

    #[test]
    fn flags_try_finally_dispose() {
        assert_eq!(run("const r = open(); try { r.read() } finally { r.dispose() }").len(), 1);
    }

    #[test]
    fn flags_try_finally_disconnect() {
        assert_eq!(run("try { query(db) } finally { db.disconnect() }").len(), 1);
    }

    #[test]
    fn allows_finally_with_multiple_statements() {
        assert!(run("try { f() } finally { cleanup(); log() }").is_empty());
    }

    #[test]
    fn allows_finally_with_non_cleanup_call() {
        assert!(run("try { f() } finally { logger.flush() }").is_empty());
    }
}
