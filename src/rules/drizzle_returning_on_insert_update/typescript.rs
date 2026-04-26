//! drizzle-returning-on-insert-update — flag `db.insert(..)` / `db.update(..)`
//! chains that mutate rows but never call `.returning()`.
//!
//! Detection: walk `call_expression` nodes. Skip anything except calls
//! whose function is a `.insert` or `.update` member expression. From
//! that node, walk outward through the chained `call_expression` /
//! `member_expression` ancestors until we reach the outermost call in
//! the chain, collecting method names along the way. If the chain
//! contains a mutation step (`.values` for insert, `.set` for update)
//! but never `.returning()`, emit a diagnostic.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    let is_insert = method == "insert";
    let is_update = method == "update";
    if !is_insert && !is_update { return; }

    // Find the outermost call expression in the chain starting at `node`.
    let (outer, methods) = collect_chain(node, source);

    // Must contain a mutation step to be a real insert/update chain.
    let has_mutation = if is_insert {
        methods.iter().any(|m| m == "values")
    } else {
        methods.iter().any(|m| m == "set")
    };
    if !has_mutation { return; }

    if methods.iter().any(|m| m == "returning") { return; }

    let pos = outer.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-returning-on-insert-update".into(),
        message: "Drizzle insert/update without `.returning()` — chain `.returning()` \
                  to get the inserted/updated row in a single round-trip."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// Starting from a `.insert(..)` / `.update(..)` call, walk up through
/// any chained `.method(..)` callers and return `(outermost_call,
/// method_names_after_start)`.
fn collect_chain<'a>(
    start: tree_sitter::Node<'a>,
    source: &[u8],
) -> (tree_sitter::Node<'a>, Vec<String>) {
    let mut methods = Vec::new();
    let mut current = start;
    while let Some(parent) = current.parent() {
        // Pattern `current.method` → member_expression whose object == current.
        if parent.kind() == "member_expression"
            && parent.child_by_field_name("object").map(|o| o.id()) == Some(current.id())
        {
            let Some(grand) = parent.parent() else { break };
            // Pattern `current.method(...)` → call whose function == parent.
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
    fn flags_insert_without_returning() {
        assert_eq!(
            run_on("await db.insert(users).values({ name: 'Alice' })").len(),
            1
        );
    }

    #[test]
    fn flags_update_without_returning() {
        assert_eq!(
            run_on("await db.update(users).set({ active: false }).where(eq(users.id, id))")
                .len(),
            1
        );
    }

    #[test]
    fn allows_insert_with_returning() {
        assert!(
            run_on("const [u] = await db.insert(users).values({ name: 'Alice' }).returning()")
                .is_empty()
        );
    }

    #[test]
    fn allows_update_with_returning() {
        assert!(run_on(
            "await db.update(users).set({ active: false }).where(eq(users.id, id)).returning()"
        )
        .is_empty());
    }

    #[test]
    fn ignores_insert_without_values() {
        // Plain `.insert(table)` with no `.values()` is incomplete, not a mutation chain.
        assert!(run_on("db.insert(users);").is_empty());
    }

    #[test]
    fn ignores_unrelated_insert() {
        // `arr.insert(0, x)` — not drizzle, but we only care about `.values()`/`.set()` shape.
        assert!(run_on("arr.insert(0, x)").is_empty());
    }
}
