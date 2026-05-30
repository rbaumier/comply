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
    // Accept any `<db>.query.<table>` shape — `db`, `database`, `tx`, `trx`,
    // `args.database`, `handle.database`, etc. are all valid Drizzle db handles.
    let Some(object) = callee.child_by_field_name("object") else {
        return false;
    };
    let obj_text = object.utf8_text(source).unwrap_or("");
    obj_text.contains(".query.")
}

crate::ast_check! { on ["call_expression"] prefilter = ["findFirst"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if !callee_is_findfirst(callee, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    // Find the first object-literal argument and check whether any of its
    // top-level properties is named `where` — covers `where: filter`,
    // shorthand `where`, and spread (`...x`, where we play safe).
    let mut has_where = false;
    let mut cursor = args.walk();
    'outer: for arg in args.named_children(&mut cursor) {
        if arg.kind() != "object" {
            continue;
        }
        // In test files, `findFirst({})` with an empty object is intentional —
        // the test wants any row without caring which one (e.g. post-import
        // assertions). An empty `{}` has no named children.
        if ctx.file.path_segments.in_test_dir && arg.named_child_count() == 0 {
            return;
        }
        let mut obj_cursor = arg.walk();
        for member in arg.named_children(&mut obj_cursor) {
            match member.kind() {
                "pair" => {
                    if let Some(key) = member.child_by_field_name("key")
                        && key.utf8_text(source).unwrap_or("") == "where"
                    {
                        has_where = true;
                        break 'outer;
                    }
                }
                "shorthand_property_identifier" => {
                    if member.utf8_text(source).unwrap_or("") == "where" {
                        has_where = true;
                        break 'outer;
                    }
                }
                "spread_element" => {
                    has_where = true;
                    break 'outer;
                }
                _ => {}
            }
        }
    }
    if has_where {
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_ts_with_file_ctx(src, &Check, &file)
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

    #[test]
    fn allows_findfirst_with_shorthand_where() {
        // Regression for #81 — `where` passed as shorthand binding.
        let src = "const u = await db.query.users.findFirst({ where, with: { posts: true } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_findfirst_with_spread() {
        let src = "const u = await db.query.users.findFirst({ ...opts });";
        assert!(run(src).is_empty());
    }

    // Regression for rbaumier/comply#357 — `database.query.*` handle with shorthand `where`.
    #[test]
    fn allows_database_handle_with_shorthand_where() {
        let src = "database.query.organization.findFirst({ where, with: { teams: true } })";
        assert!(run(src).is_empty());
    }

    // Regression for rbaumier/comply#357 — nested handle `args.database.query.*`.
    #[test]
    fn allows_nested_database_handle_with_shorthand_where() {
        let src = "args.database.query.team.findFirst({ where, columns: { id: true } })";
        assert!(run(src).is_empty());
    }

    // Regression for rbaumier/comply#357 — `database.query.*` without `where` must be flagged.
    #[test]
    fn flags_database_handle_without_where() {
        let src = "database.query.organization.findFirst({ columns: { id: true } })";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for rbaumier/comply#530 — `findFirst({})` with empty object in test files is
    // intentional (fetch any row for post-import assertions).
    #[test]
    fn no_fp_findfirst_empty_object_in_test_file() {
        let src = "const anyRow = await db.query.team.findFirst({});";
        assert!(run_in_test_file(src).is_empty());
    }

    // `findFirst({})` in production code is still flagged.
    #[test]
    fn flags_findfirst_empty_object_in_production() {
        let src = "const anyRow = await db.query.team.findFirst({});";
        assert_eq!(run(src).len(), 1);
    }

    // `findFirst({ columns: {...} })` without `where` in test files is still flagged.
    #[test]
    fn flags_findfirst_with_options_no_where_in_test_file() {
        let src = "const u = await db.query.users.findFirst({ columns: { id: true } });";
        assert_eq!(run_in_test_file(src).len(), 1);
    }
}
