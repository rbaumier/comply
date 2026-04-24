//! Within a function body (`statement_block`), flag when 2+
//! `db.insert`/`db.update`/`db.delete` call_expressions appear and the
//! enclosing function is not a `db.transaction(...)` callback.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if `node` is inside the callback argument of a `db.transaction(...)`
/// call.
fn is_in_transaction_callback(node: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "call_expression" {
            let func = parent.child_by_field_name("function");
            if let Some(f) = func {
                let text = f.utf8_text(src).unwrap_or("");
                if text.ends_with(".transaction") || text == "transaction" {
                    return true;
                }
            }
        }
        cur = parent;
    }
    false
}

fn is_db_mutation_call<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    let name = prop.utf8_text(src).unwrap_or("");
    matches!(name, "insert" | "update" | "delete")
}

/// Walk down a chained call expression to find any inner `db.insert/update/delete`
/// call. For `db.insert(users).values({...})`, the outer call's function's
/// object is the `db.insert(users)` call we care about.
fn chain_contains_mutation(node: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    let mut cur = node;
    loop {
        if cur.kind() != "call_expression" {
            return false;
        }
        if is_db_mutation_call(&cur, src) {
            return true;
        }
        let Some(func) = cur.child_by_field_name("function") else {
            return false;
        };
        if func.kind() != "member_expression" {
            return false;
        }
        let Some(obj) = func.child_by_field_name("object") else {
            return false;
        };
        cur = obj;
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "statement_block" {
        return;
    }
    // Count mutation calls at the top level of this block (direct
    // expression_statements).
    let mut cursor = node.walk();
    let mut mutation_nodes: Vec<tree_sitter::Node<'_>> = Vec::new();
    for stmt in node.children(&mut cursor) {
        if stmt.kind() != "expression_statement" {
            continue;
        }
        // The expression could be an await_expression wrapping the call.
        // Strip await_expression layers until we hit the underlying call.
        let mut expr_opt = stmt.child(0);
        while let Some(e) = expr_opt {
            if e.kind() == "await_expression" {
                // await_expression wraps a single expression; find the first
                // call_expression child.
                let mut found: Option<tree_sitter::Node<'_>> = None;
                let mut c = e.walk();
                for ch in e.children(&mut c) {
                    if ch.kind() == "call_expression" {
                        found = Some(ch);
                        break;
                    }
                }
                expr_opt = found;
                continue;
            }
            break;
        }
        let Some(expr) = expr_opt else { continue };
        if expr.kind() != "call_expression" {
            continue;
        }
        if !chain_contains_mutation(expr, source) {
            continue;
        }
        mutation_nodes.push(expr);
    }
    if mutation_nodes.len() < 2 {
        return;
    }
    if is_in_transaction_callback(node, source) {
        return;
    }
    // Flag the first mutation node.
    let first = mutation_nodes[0];
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &first,
        super::META.id,
        "Sequential `db.insert`/`db.update`/`db.delete` calls in the same scope — wrap them in `db.transaction(async (tx) => { ... })` so partial failures roll back.".into(),
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
    fn flags_two_sequential_mutations() {
        let src = "async function f() {\n  await db.insert(users).values({ id: 1 });\n  await db.update(posts).set({ x: 1 });\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inside_transaction() {
        let src = "async function f() { await db.transaction(async (tx) => {\n  await tx.insert(users).values({ id: 1 });\n  await tx.update(posts).set({ x: 1 });\n}); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_mutation() {
        let src = "async function f() { await db.insert(users).values({ id: 1 }); }";
        assert!(run(src).is_empty());
    }
}
