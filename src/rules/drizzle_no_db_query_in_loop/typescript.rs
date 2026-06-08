//! drizzle-no-db-query-in-loop — flag a `db.select(...)`/`db.insert(...)`/
//! `db.update(...)`/`db.delete(...)`/`db.query.<name>.<...>` call that has
//! a `for_statement`, `for_in_statement`, `for_of_statement`, or
//! `.map(...)` / `.forEach(...)` ancestor.

use crate::diagnostic::{Diagnostic, Severity};

const DB_METHODS: &[&str] = &["select", "insert", "update", "delete"];
const ARRAY_LOOP_METHODS: &[&str] = &["map", "forEach"];

fn callee_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<String> {
    let callee = node.child_by_field_name("function")?;
    Some(callee.utf8_text(source).unwrap_or("").to_string())
}

fn is_db_query(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let text = callee.utf8_text(source).unwrap_or("");
    if let Some(prop) = callee.child_by_field_name("property") {
        let prop_name = prop.utf8_text(source).unwrap_or("");
        if let Some(obj) = callee.child_by_field_name("object") {
            let obj_text = obj.utf8_text(source).unwrap_or("");
            // db.select / db.insert / db.update / db.delete
            if obj_text == "db" && DB_METHODS.contains(&prop_name) {
                return true;
            }
            // db.query.users.findMany etc.
            if obj_text.starts_with("db.query.") || obj_text == "db.query" {
                return true;
            }
        }
        // tx.select(...) / trx.select(...) — common transaction handles.
        if (text.starts_with("tx.") || text.starts_with("trx.")) && DB_METHODS.contains(&prop_name)
        {
            return true;
        }
    }
    false
}

fn loop_ancestor(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "for_statement" | "for_in_statement" | "for_of_statement" | "while_statement" => {
                return true;
            }
            "call_expression" => {
                if let Some(text) = callee_text(parent, source) {
                    let prop = text.rsplit('.').next().unwrap_or("");
                    if ARRAY_LOOP_METHODS.contains(&prop) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        cur = parent;
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["db.query", "tx.query", "trx.query"] => |node, source, ctx, diagnostics|
    if !is_db_query(node, source) {
        return;
    }
    if !loop_ancestor(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-no-db-query-in-loop".into(),
        message: "Drizzle query inside a loop / `.map` / `.forEach` causes N+1 round-trips — batch with `inArray(...)` or join instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_select_in_for_of() {
        let src =
            "for (const id of ids) { await db.select().from(users).where(eq(users.id, id)); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_select_in_map() {
        let src = "ids.map((id) => db.select().from(users).where(eq(users.id, id)));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_query_findfirst_in_foreach() {
        let src = "ids.forEach((id) => db.query.users.findFirst({ where: eq(users.id, id) }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_select_outside_loop() {
        let src = "await db.select().from(users).where(inArray(users.id, ids));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_db_call_in_loop() {
        let src = "for (const id of ids) { logger.info(id); }";
        assert!(run(src).is_empty());
    }
}
