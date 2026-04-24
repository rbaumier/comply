//! Flag `.leftJoin(` and `.innerJoin(` method calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let name = prop.utf8_text(source).unwrap_or("");
    if name != "leftJoin" && name != "innerJoin" && name != "rightJoin" && name != "fullJoin" {
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
    fn flags_left_join() {
        let src = "const r = db.select().from(users).leftJoin(posts, eq(users.id, posts.userId))";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inner_join() {
        let src = "const r = db.select().from(users).innerJoin(posts, eq(users.id, posts.userId))";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_query_findmany_with() {
        let src = "const r = db.query.users.findMany({ with: { posts: true } })";
        assert!(run(src).is_empty());
    }
}
