//! enforce-delete-with-where — flag `db.delete(table)` chains that have
//! no `.where(...)` call anywhere in the chain.
//!
//! Detection: walk `call_expression` nodes whose function is a
//! `member_expression` with property `delete` and whose receiver is
//! plausibly a database client (`db`, `database`, `tx`, `trx`, or any
//! identifier containing `db` / `database`). From that call, walk
//! outward through chained `.method(...)` ancestors collecting method
//! names. If the chain contains no `.where`, emit one diagnostic
//! anchored on the outer call.
//!
//! The receiver filter keeps `Map.prototype.delete` / `Set.prototype.delete`
//! / cache `.delete()` calls out of the noise floor while still catching
//! the common drizzle / knex / kysely patterns.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = [".delete("] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "delete" { return; }

    let Some(obj) = func.child_by_field_name("object") else { return };
    if !receiver_looks_like_db(obj, source) { return; }

    let (outer, methods) = collect_chain(node, source);

    if methods.iter().any(|m| m == "where") { return; }

    let pos = outer.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "enforce-delete-with-where".into(),
        message: "`db.delete(...)` without `.where(...)` removes every row in the table — add a \
                  `.where(condition)` clause to bound the deletion."
            .into(),
        severity: Severity::Error,
        span: Some((outer.byte_range().start, outer.byte_range().len())),
    });
}

/// Decide whether the receiver of `.delete(..)` looks like a database
/// client. We accept identifiers whose lowercased name is `db`,
/// `database`, `tx`, `trx`, `conn`, `client`, `drizzle` or contains
/// `db` / `database` as a substring (e.g. `userDb`, `myDatabase`).
/// For member expressions, we inspect the leftmost identifier (e.g.
/// `this.db` → `db`).
fn receiver_looks_like_db(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let name = leftmost_identifier(node, source);
    let Some(name) = name else { return false };
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        "db" | "database" | "tx" | "trx" | "conn" | "client" | "drizzle"
    ) || lower.contains("db")
        || lower.contains("database")
}

fn leftmost_identifier(mut node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    loop {
        match node.kind() {
            "identifier" | "property_identifier" | "shorthand_property_identifier" => {
                return node.utf8_text(source).ok().map(ToOwned::to_owned);
            }
            "member_expression" => {
                // Prefer the property — `this.db` should resolve to `db`.
                if let Some(prop) = node.child_by_field_name("property")
                    && let Ok(txt) = prop.utf8_text(source)
                {
                    return Some(txt.to_owned());
                }
                let obj = node.child_by_field_name("object")?;
                node = obj;
            }
            "this" => return Some("this".into()),
            _ => return None,
        }
    }
}

/// Starting from a `.delete(..)` call, walk up the chain collecting
/// subsequent method names (`.where`, `.returning`, etc.) and return
/// `(outermost_call, method_names)`.
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_delete_without_where() {
        assert_eq!(run_on("await db.delete(usersTable)").len(), 1);
    }

    #[test]
    fn flags_delete_with_returning_but_no_where() {
        assert_eq!(run_on("await db.delete(usersTable).returning()").len(), 1);
    }

    #[test]
    fn allows_delete_with_where() {
        assert!(run_on("await db.delete(usersTable).where(eq(usersTable.id, 1))").is_empty());
    }

    #[test]
    fn allows_delete_with_where_and_returning() {
        assert!(
            run_on("await db.delete(usersTable).where(eq(usersTable.id, 1)).returning()")
                .is_empty()
        );
    }

    #[test]
    fn ignores_map_delete() {
        // Receiver isn't a plausible db handle — Map/Set/cache cleanup
        // must not trip this rule.
        assert!(run_on("const m = new Map(); m.delete('k')").is_empty());
    }

    #[test]
    fn flags_tx_delete_without_where() {
        assert_eq!(run_on("await tx.delete(usersTable)").len(), 1);
    }
}
