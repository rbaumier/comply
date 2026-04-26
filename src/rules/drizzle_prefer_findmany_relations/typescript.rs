//! Flag `.leftJoin(` / `.innerJoin(` / `.rightJoin(` / `.fullJoin(`
//! method calls — but only in files that also define or import
//! `relations(`. Without relations defined, `findMany({ with })` is not
//! an option, so the manual join is the only choice.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let name = prop.utf8_text(source).unwrap_or("");
    if name != "leftJoin" && name != "innerJoin" && name != "rightJoin" && name != "fullJoin" {
        return;
    }
    // Only warn when the file actually has Drizzle relations available
    // — either defined locally or imported. Looking for the literal
    // `relations(` call form catches both `import { relations } from
    // 'drizzle-orm'` (followed by a call) and `export const xRelations =
    // relations(table, …)`.
    if !ctx.source.contains("relations(") {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Manual `.{name}(...)` chain — prefer `db.query.X.findMany({{ with: {{ ... }} }})` when relations are defined."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_left_join_with_relations_defined() {
        let src = "export const userRelations = relations(users, ({ many }) => ({ posts: many(posts) }));\n\
                   const r = db.select().from(users).leftJoin(posts, eq(users.id, posts.userId))";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inner_join_with_relations_defined() {
        let src = "export const userRelations = relations(users, ({ many }) => ({ posts: many(posts) }));\n\
                   const r = db.select().from(users).innerJoin(posts, eq(users.id, posts.userId))";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_query_findmany_with() {
        let src = "const r = db.query.users.findMany({ with: { posts: true } })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_join_when_no_relations_defined() {
        // REVIEW regression: without `relations()` in the file, the
        // manual join is the only option — don't flag it.
        let src = "const r = db.select().from(users).leftJoin(posts, eq(users.id, posts.userId))";
        assert!(run(src).is_empty());
    }
}
