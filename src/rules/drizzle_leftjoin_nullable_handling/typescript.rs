//! drizzle-leftjoin-nullable-handling — when a Drizzle chain uses
//! `.leftJoin(<table>, ...)`, flag the call if no `null` / `?? ` /
//! `?.` handling is visible textually within the surrounding statement.
//! Exception: explicit select object (`select({`) combined with a Zod
//! `.nullable()` declaration elsewhere in the file signals that the joined
//! field is intentionally nullable per the response schema.

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
    // When the statement uses an explicit select object AND the file declares
    // nullable fields (e.g. Zod `.nullable()`), the developer intentionally
    // projected a nullable field from the joined table — not a bug.
    if stmt_text.contains("select({") && ctx.source_contains(".nullable()") {
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

    // Issue #527: explicit select object + Zod .nullable() schema = intentionally nullable
    #[test]
    fn no_fp_when_explicit_select_and_nullable_schema_in_file() {
        let src = r#"
const responseSchema = z.object({ extraField: z.string().nullable() });
const rows = db
  .select({
    id: mainTable.id,
    extraField: joinedView.someNullableField,
  })
  .from(mainTable)
  .leftJoin(joinedView, eq(joinedView.id, mainTable.joinedId));
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_wildcard_select_even_with_nullable_schema_in_file() {
        let src = r#"
const schema = z.object({ name: z.string().nullable() });
const rows = await db.select().from(users).leftJoin(posts, eq(posts.userId, users.id));
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_explicit_select_without_nullable_schema_in_file() {
        let src = "const rows = db.select({ userId: posts.userId }).from(users).leftJoin(posts, eq(posts.userId, users.id));";
        assert_eq!(run(src).len(), 1);
    }
}
