use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const CLEANUP_METHODS: &[&str] =
    &["close", "dispose", "destroy", "disconnect", "release", "end"];

const KINDS: &[&str] = &["try_statement"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
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
        if !CLEANUP_METHODS.contains(&method) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Use `using` / `await using` instead of try/finally with `.{method}()` (TS 5.2+)."
            ),
            severity: Severity::Warning,
            span: None,
        });
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
