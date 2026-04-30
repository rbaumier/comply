//! Detect calls to `pgTable` / `mysqlTable` / `sqliteTable` whose column
//! definition object has exactly 2 entries, each chaining `.references(`,
//! and whose full call text does not contain `primaryKey(`.

use crate::diagnostic::{Diagnostic, Severity};

const TABLE_CTORS: &[&str] = &["pgTable", "mysqlTable", "sqliteTable"];

fn callee_name<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> Option<&'a str> {
    let func = node.child_by_field_name("function")?;
    if func.kind() == "identifier" {
        return func.utf8_text(src).ok();
    }
    None
}

/// Given the `arguments` node, return the object argument (the columns
/// definition) — typically the second argument.
fn columns_object<'a>(args: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = args.walk();
    let mut objects: Vec<tree_sitter::Node<'_>> = Vec::new();
    for c in args.children(&mut cursor) {
        if c.kind() == "object" {
            objects.push(c);
        }
    }
    // The columns object is the first object arg.
    objects.into_iter().next()
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = callee_name(&node, source) else { return };
    if !TABLE_CTORS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(cols) = columns_object(args) else { return };

    // Count pair children that chain `.references(`.
    let mut cursor = cols.walk();
    let mut pair_count = 0usize;
    let mut fk_count = 0usize;
    for child in cols.children(&mut cursor) {
        if child.kind() != "pair" { continue; }
        pair_count += 1;
        let text = child.utf8_text(source).unwrap_or("");
        if text.contains(".references(") {
            fk_count += 1;
        }
    }

    if pair_count != 2 || fk_count != 2 {
        return;
    }

    let full = node.utf8_text(source).unwrap_or("");
    if full.contains("primaryKey(") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Junction table (2 FK columns) must declare a composite `primaryKey({ columns: [...] })` in the table options callback.".into(),
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
    fn flags_junction_without_pk() {
        let src = "const t = pgTable('users_roles', {\n  userId: integer('user_id').references(() => users.id),\n  roleId: integer('role_id').references(() => roles.id),\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_junction_with_composite_pk() {
        let src = "const t = pgTable('users_roles', {\n  userId: integer('user_id').references(() => users.id),\n  roleId: integer('role_id').references(() => roles.id),\n}, (t) => ({ pk: primaryKey({ columns: [t.userId, t.roleId] }) }))";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_junction() {
        let src =
            "const t = pgTable('users', { id: serial('id').primaryKey(), name: text('name') })";
        assert!(run(src).is_empty());
    }
}
