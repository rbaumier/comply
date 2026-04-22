//! drizzle-no-select-without-limit — flag `db.select().from(table)`
//! chains that have no `.limit(..)` and no `.where(..)`.
//!
//! Detection: walk `call_expression` nodes whose function is `.select`
//! property on any object. From that call, walk outward through chained
//! `.method(..)` ancestors collecting method names. If the chain
//! contains `.from` but no `.limit` and no `.where`, emit one
//! diagnostic anchored on the outer call.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "select" { return; }

    let (outer, methods) = collect_chain(node, source);

    if !methods.iter().any(|m| m == "from") { return; }
    if methods.iter().any(|m| m == "limit" || m == "where") { return; }

    let pos = outer.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-no-select-without-limit".into(),
        message: "`db.select().from(table)` without `.limit()` or `.where()` scans the \
                  entire table — add a bound to avoid loading unbounded rows."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// Starting from a `.select(..)` call, walk up the chain collecting
/// subsequent method names (`.from`, `.where`, `.limit`, etc.) and
/// return `(outermost_call, method_names)`.
fn collect_chain<'a>(
    start: tree_sitter::Node<'a>,
    source: &[u8],
) -> (tree_sitter::Node<'a>, Vec<String>) {
    let mut methods = Vec::new();
    let mut current = start;
    while let Some(parent) = current.parent() {
        if parent.kind() == "member_expression"
            && parent.child_by_field_name("object").map(|o| o.id()) == Some(current.id())
        {
            let Some(grand) = parent.parent() else { break };
            if grand.kind() == "call_expression"
                && grand.child_by_field_name("function").map(|f| f.id()) == Some(parent.id())
            {
                if let Some(prop) = parent.child_by_field_name("property") {
                    methods.push(prop.utf8_text(source).unwrap_or("").to_string());
                }
                current = grand;
                continue;
            }
        }
        break;
    }
    (current, methods)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_unbounded_select() {
        assert_eq!(
            run_on("const users = await db.select().from(usersTable)").len(),
            1
        );
    }

    #[test]
    fn flags_partial_select_without_limit() {
        assert_eq!(
            run_on("const all = await db.select({ id: users.id }).from(usersTable)").len(),
            1
        );
    }

    #[test]
    fn allows_select_with_where() {
        assert!(
            run_on("await db.select().from(usersTable).where(eq(usersTable.active, true))")
                .is_empty()
        );
    }

    #[test]
    fn allows_select_with_limit() {
        assert!(run_on("await db.select().from(usersTable).limit(20)").is_empty());
    }

    #[test]
    fn ignores_select_without_from() {
        // Something other than a query builder — still `.select(..)` but no `.from()`.
        assert!(run_on("obj.select(x)").is_empty());
    }

    #[test]
    fn allows_select_with_where_before_from() {
        // Chain order shouldn't matter for detection.
        assert!(
            run_on("await db.select().from(usersTable).where(eq(usersTable.id, 1)).limit(1)")
                .is_empty()
        );
    }
}
