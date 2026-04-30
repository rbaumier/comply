//! drizzle-leftjoin-nullable-handling — when a Drizzle chain uses
//! `.leftJoin(<table>, ...)`, flag the call if no `null` / `?? ` /
//! `?.` handling is visible textually within the surrounding statement.

use crate::diagnostic::{Diagnostic, Severity};

fn enclosing_statement<'a>(node: tree_sitter::Node<'a>) -> tree_sitter::Node<'a> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if matches!(
            parent.kind(),
            "expression_statement"
                | "variable_declarator"
                | "lexical_declaration"
                | "return_statement"
        ) {
            return parent;
        }
        cur = parent;
    }
    cur
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "leftJoin" {
        return;
    }
    let stmt = enclosing_statement(node);
    let stmt_text = stmt.utf8_text(source).unwrap_or("");
    // Heuristic: any explicit null-awareness signal nearby is enough to silence.
    if stmt_text.contains("?.")
        || stmt_text.contains("?? ")
        || stmt_text.contains("=== null")
        || stmt_text.contains("!== null")
        || stmt_text.contains("isNotNull(")
        || stmt_text.contains("if (") && stmt_text.contains("!= null")
    {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-leftjoin-nullable-handling".into(),
        message: "`.leftJoin(...)` produces nullable joined columns — handle `null` (filter, `??`, or `isNotNull`) before reading the joined fields.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_leftjoin_without_null_check() {
        let src = "const rows = await db.select().from(users).leftJoin(posts, eq(posts.userId, users.id));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_leftjoin_with_isnotnull() {
        let src = "const rows = await db.select().from(users).leftJoin(posts, eq(posts.userId, users.id)).where(isNotNull(posts.id));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_leftjoin_with_optional_chain_consumer() {
        let src = "const rows = await db.select().from(users).leftJoin(posts, eq(posts.userId, users.id)).then((r) => r?.map((x) => x));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_innerjoin() {
        let src = "const rows = await db.select().from(users).innerJoin(posts, eq(posts.userId, users.id));";
        assert!(run(src).is_empty());
    }
}
