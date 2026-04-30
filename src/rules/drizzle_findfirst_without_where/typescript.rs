//! drizzle-findfirst-without-where — flag `db.query.<table>.findFirst()` /
//! `db.query.<table>.findFirst({ ... })` whose options don't include `where:`.

use crate::diagnostic::{Diagnostic, Severity};

fn callee_is_findfirst(callee: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    if prop.utf8_text(source).unwrap_or("") != "findFirst" {
        return false;
    }
    // Object should look like `db.query.<table>` to keep this Drizzle-specific.
    let Some(object) = callee.child_by_field_name("object") else {
        return false;
    };
    let obj_text = object.utf8_text(source).unwrap_or("");
    obj_text.starts_with("db.query.")
        || obj_text.starts_with("tx.query.")
        || obj_text.starts_with("trx.query.")
}

crate::ast_check! { on ["call_expression"] prefilter = ["findFirst"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if !callee_is_findfirst(callee, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let text = args.utf8_text(source).unwrap_or("");
    if text.contains("where:") || text.contains("where :") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-findfirst-without-where".into(),
        message: "`.findFirst()` without `where:` returns an arbitrary row — pass a filter to scope the query.".into(),
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
    fn flags_findfirst_no_args() {
        let src = "const u = await db.query.users.findFirst();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_findfirst_with_options_but_no_where() {
        let src = "const u = await db.query.users.findFirst({ columns: { id: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_findfirst_with_where() {
        let src = "const u = await db.query.users.findFirst({ where: eq(users.id, id) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_other_findfirst_objects() {
        let src = "arr.findFirst();";
        assert!(run(src).is_empty());
    }
}
