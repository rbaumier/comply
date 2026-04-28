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

const DB_RECEIVERS: &[&str] = &["db", "tx", "drizzle", "orm", "conn", "connection", "client"];

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
    if !matches!(name, "insert" | "update" | "delete") {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    let obj_text = obj.utf8_text(src).unwrap_or("");
    DB_RECEIVERS.iter().any(|r| obj_text == *r)
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

/// Recursively collect mutation calls in `block`, descending one level
/// into `if_statement`/`else` blocks (and similar simple control-flow
/// containers). Loops and nested function bodies are NOT descended into
/// — those represent a different scope.
fn collect_block_mutations<'a>(
    block: tree_sitter::Node<'a>,
    source: &[u8],
    out: &mut Vec<tree_sitter::Node<'a>>,
) {
    let mut cursor = block.walk();
    for stmt in block.children(&mut cursor) {
        match stmt.kind() {
            "expression_statement" => {
                if let Some(expr) = strip_to_call(stmt.child(0))
                    && chain_contains_mutation(expr, source)
                {
                    out.push(expr);
                }
            }
            // if (cond) { … } else { … } / else if (…) { … }
            "if_statement" => {
                let mut c = stmt.walk();
                for ch in stmt.children(&mut c) {
                    if ch.kind() == "statement_block" {
                        collect_block_mutations(ch, source, out);
                    } else if ch.kind() == "else_clause" {
                        let mut ec = ch.walk();
                        for inner in ch.children(&mut ec) {
                            if inner.kind() == "statement_block" {
                                collect_block_mutations(inner, source, out);
                            } else if inner.kind() == "if_statement" {
                                // else if — recurse via a synthetic block walk.
                                let mut ic = inner.walk();
                                for grand in inner.children(&mut ic) {
                                    if grand.kind() == "statement_block" {
                                        collect_block_mutations(grand, source, out);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // try { … } catch { … } / try { … } finally { … }
            "try_statement" => {
                let mut c = stmt.walk();
                for ch in stmt.children(&mut c) {
                    if ch.kind() == "statement_block" {
                        collect_block_mutations(ch, source, out);
                    } else if matches!(ch.kind(), "catch_clause" | "finally_clause") {
                        let mut ec = ch.walk();
                        for inner in ch.children(&mut ec) {
                            if inner.kind() == "statement_block" {
                                collect_block_mutations(inner, source, out);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Strip `await_expression` layers from an expression node to reach
/// the underlying call expression.
fn strip_to_call(mut expr_opt: Option<tree_sitter::Node<'_>>) -> Option<tree_sitter::Node<'_>> {
    while let Some(e) = expr_opt {
        if e.kind() == "await_expression" {
            let mut c = e.walk();
            let mut found: Option<tree_sitter::Node<'_>> = None;
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
    let expr = expr_opt?;
    if expr.kind() != "call_expression" {
        return None;
    }
    Some(expr)
}

crate::ast_check! { on ["statement_block"] => |node, source, ctx, diagnostics|
    // Skip nested blocks: only the outermost function/transaction body
    // should drive a single diagnostic. We rely on the parent kind: if
    // the block's parent is itself an inner control-flow node we let
    // the outer block do the counting.
    if let Some(parent) = node.parent()
        && matches!(
            parent.kind(),
            "if_statement"
                | "else_clause"
                | "try_statement"
                | "catch_clause"
                | "finally_clause"
        )
    {
        return;
    }

    let mut mutation_nodes: Vec<tree_sitter::Node<'_>> = Vec::new();
    collect_block_mutations(node, source, &mut mutation_nodes);

    if mutation_nodes.len() < 2 {
        return;
    }
    if is_in_transaction_callback(node, source) {
        return;
    }
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

    #[test]
    fn flags_mutations_split_across_if_else() {
        // REVIEW regression: mutations buried inside if/else branches
        // were missed by the previous direct-children-only count.
        let src = "async function f(c) {\n  \
                   if (c) {\n    await db.insert(users).values({ id: 1 });\n  } else {\n    await db.update(posts).set({ x: 1 });\n  }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_top_level_then_nested_mutation() {
        let src = "async function f(c) {\n  \
                   await db.insert(users).values({ id: 1 });\n  \
                   if (c) { await db.update(posts).set({ x: 1 }); }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_hmac_update() {
        let src = "function sign() {\n  hmac.update(Buffer.from(ts));\n  hmac.update(sep);\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_cache_update() {
        let src = "function f() {\n  cache.update(k, v);\n  cache.delete(old);\n}";
        assert!(run(src).is_empty());
    }
}
