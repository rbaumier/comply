//! drizzle-prefer-select-columns — flag `db.select()` (no argument) and
//! `tx.select()` / `trx.select()` that are followed by `.from(...)` in the
//! chain.

use crate::diagnostic::{Diagnostic, Severity};

fn select_caller_is_db(callee: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    if prop.utf8_text(source).unwrap_or("") != "select" {
        return false;
    }
    let Some(object) = callee.child_by_field_name("object") else {
        return false;
    };
    let obj_text = object.utf8_text(source).unwrap_or("");
    matches!(obj_text, "db" | "tx" | "trx")
}

/// Walk outward from a `.select()` call collecting the chained method names.
fn collect_chain<'a>(start: tree_sitter::Node<'a>, source: &[u8]) -> Vec<String> {
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
    methods
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    if !select_caller_is_db(callee, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    if args.named_children(&mut cursor).next().is_some() {
        return; // has at least one argument
    }
    let methods = collect_chain(node, source);
    if !methods.iter().any(|m| m == "from") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-prefer-select-columns".into(),
        message: "`db.select()` with no projection fetches every column — pass `{ col: table.col, ... }` to scope the read.".into(),
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
    fn flags_select_no_args() {
        let src = "await db.select().from(users).limit(1);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_tx_select_no_args() {
        let src = "await tx.select().from(users).where(eq(users.id, 1));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_select_with_columns() {
        let src = "await db.select({ id: users.id }).from(users);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_other_select_callers() {
        let src = "obj.select().from(users);";
        assert!(run(src).is_empty());
    }
}
