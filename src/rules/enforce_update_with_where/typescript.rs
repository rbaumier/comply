//! enforce-update-with-where — flag `db.update(table)` chains that have
//! no `.where(...)` call anywhere in the chain.
//!
//! Detection: walk `call_expression` nodes whose function is a
//! `member_expression` with property `update` and whose receiver looks
//! like a database client. From that call, walk outward through chained
//! `.method(..)` ancestors collecting method names. If the chain
//! contains no `.where`, emit one diagnostic anchored on the outer call.
//!
//! The receiver filter (`db`, `database`, `tx`, `trx`, `conn`,
//! `client`, `drizzle`, or any identifier containing `db`/`database`)
//! keeps state-setter / React-query / generic `.update(..)` calls out
//! of the noise floor.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "update" { return; }

    let Some(obj) = func.child_by_field_name("object") else { return };
    if !receiver_looks_like_db(obj, source) { return; }

    let (outer, methods) = collect_chain(node, source);

    if methods.iter().any(|m| m == "where") { return; }

    let pos = outer.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "enforce-update-with-where".into(),
        message: "`db.update(...)` without `.where(...)` updates every row in the table — add a \
                  `.where(condition)` clause to bound the update."
            .into(),
        severity: Severity::Error,
        span: Some((outer.byte_range().start, outer.byte_range().len())),
    });
}

/// Decide whether the receiver of `.update(..)` looks like a database
/// client. See `enforce-delete-with-where` for the identical rationale
/// and allowlist.
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
    fn flags_update_without_where() {
        assert_eq!(
            run_on("await db.update(usersTable).set({ active: true })").len(),
            1
        );
    }

    #[test]
    fn flags_update_set_returning_without_where() {
        assert_eq!(
            run_on(
                "await db.update(usersTable).set({ active: true }).returning()"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_update_with_where() {
        assert!(
            run_on(
                "await db.update(usersTable).set({ active: true }).where(eq(usersTable.id, 1))"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_unrelated_update() {
        // State setter / react-query mutate / generic `.update(..)` must
        // not be flagged.
        assert!(run_on("store.update({ count: 1 })").is_empty());
    }

    #[test]
    fn flags_tx_update_without_where() {
        assert_eq!(
            run_on("await tx.update(usersTable).set({ active: true })").len(),
            1
        );
    }
}
