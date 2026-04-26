//! Flag `.prepare(` call chains where a `.where(...)` appears and the
//! chain text does not contain `sql.placeholder(` / `placeholder(`.

use crate::diagnostic::{Diagnostic, Severity};

/// Walk up the chain to the outermost call_expression starting at `node`.
fn chain_root(start: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let mut cur = start;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "member_expression" | "call_expression" => cur = parent,
            _ => break,
        }
    }
    cur
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "prepare" {
        return;
    }
    // We found a `.prepare(...)` call. Walk to the full chain root.
    let root = chain_root(node);
    let chain = root.utf8_text(source).unwrap_or("");
    if !chain.contains(".where(") {
        return;
    }
    if chain.contains("placeholder(") {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.prepare()` with `.where(...)` must use `sql.placeholder('name')` instead of inline variables so the prepared statement can be reused across executions.".into(),
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
    fn flags_prepare_with_inline_where() {
        let src = "const q = db.select().from(u).where(eq(u.id, id)).prepare('q')";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_prepare_with_placeholder() {
        let src = "const q = db.select().from(u).where(eq(u.id, sql.placeholder('id'))).prepare('q')";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_prepare_without_where() {
        let src = "const q = db.select().from(u).prepare('q')";
        assert!(run(src).is_empty());
    }
}
