//! db-no-n-plus-one — flag `await db.query(...)` inside loops.
//!
//! Walks the AST looking for await expressions containing DB call patterns
//! inside for/forEach/map loops.

use crate::diagnostic::{Diagnostic, Severity};

const QUERY_METHODS: &[&str] = &[
    "query",
    "execute",
    "findFirst",
    "findMany",
    "findUnique",
    "create",
    "update",
    "delete",
];

/// Node kinds that represent loops.
const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];

crate::ast_check! { on ["await_expression"] => |node, source, ctx, diagnostics|
    // Look for await_expression nodes.
    // Check if the awaited expression is a DB call.
    let Some(arg) = node.named_child(0) else { return };
    if !is_db_call(&arg, source) {
        return;
    }

    // Walk up the tree to see if we're inside a loop.
    let mut parent = node.parent();
    while let Some(p) = parent {
        if LOOP_KINDS.contains(&p.kind()) {
            let pos = node.start_position();
            let loop_line = p.start_position().row + 1;
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "db-no-n-plus-one".into(),
                message: format!(
                    "N+1 query: `await` + DB call inside a loop (started at line \
                     {loop_line}). Use a JOIN, `WHERE id IN (...)`, or batch fetch."
                ),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        // Also detect `.forEach(async ...)` / `.map(async ...)`
        if p.kind() == "call_expression"
            && let Some(func) = p.child_by_field_name("function")
                && func.kind() == "member_expression"
                    && let Some(prop) = func.child_by_field_name("property") {
                        let name = prop.utf8_text(source).unwrap_or("");
                        if name == "forEach" || name == "map" {
                            let pos = node.start_position();
                            let loop_line = p.start_position().row + 1;
                            diagnostics.push(Diagnostic {
                                path: std::sync::Arc::clone(&ctx.path_arc),
                                line: pos.row + 1,
                                column: pos.column + 1,
                                rule_id: "db-no-n-plus-one".into(),
                                message: format!(
                                    "N+1 query: `await` + DB call inside a loop \
                                     (started at line {loop_line}). Use a JOIN, \
                                     `WHERE id IN (...)`, or batch fetch."
                                ),
                                severity: Severity::Error,
                                span: None,
                            });
                            return;
                        }
                    }

        parent = p.parent();
    }
}

/// Check if a node is a DB-related call expression.
fn is_db_call(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() == "member_expression" {
        // Check method name: db.query, prisma.findMany, etc.
        if let Some(prop) = func.child_by_field_name("property") {
            let method = prop.utf8_text(source).unwrap_or("");
            if QUERY_METHODS.contains(&method) {
                return true;
            }
        }
        // Check object name: db.*, prisma.*, drizzle.*
        if let Some(obj) = func.child_by_field_name("object") {
            let obj_text = obj.utf8_text(source).unwrap_or("");
            if obj_text == "db" || obj_text == "prisma" || obj_text == "drizzle" {
                return true;
            }
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
    fn flags_await_in_for_loop() {
        let s = "for (const u of users) {\n  const orders = await db.query('SELECT * FROM orders WHERE user_id = $1', [u.id]);\n}";
        assert_eq!(run_on(s).len(), 1);
    }

    #[test]
    fn flags_await_in_for_each() {
        let s = "users.forEach(async (u) => {\n  await prisma.findMany({ where: { userId: u.id } });\n});";
        assert_eq!(run_on(s).len(), 1);
    }

    #[test]
    fn allows_batch_query() {
        assert!(run_on("const orders = await db.query('SELECT * FROM orders WHERE user_id IN ($1)', [ids]);").is_empty());
    }
}
