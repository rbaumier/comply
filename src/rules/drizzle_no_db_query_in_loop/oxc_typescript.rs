//! drizzle-no-db-query-in-loop OxcCheck backend — flag Drizzle query calls
//! that have a loop ancestor.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const DB_METHODS: &[&str] = &["select", "insert", "update", "delete"];
const ARRAY_LOOP_METHODS: &[&str] = &["map", "forEach"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["db.", "tx.", "trx."])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if !is_db_query(call, ctx.source) {
            return;
        }

        if !has_loop_ancestor(node, semantic, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Drizzle query inside a loop / `.map` / `.forEach` causes N+1 round-trips — batch with `inArray(...)` or join instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_db_query(call: &oxc_ast::ast::CallExpression, source: &str) -> bool {
    let Expression::StaticMemberExpression(mem) = &call.callee else {
        return false;
    };
    let prop = mem.property.name.as_str();
    let obj_text = &source[mem.object.span().start as usize..mem.object.span().end as usize];

    // db.select / db.insert / db.update / db.delete
    if obj_text == "db" && DB_METHODS.contains(&prop) {
        return true;
    }
    // db.query.users.findMany etc.
    if obj_text.starts_with("db.query.") || obj_text == "db.query" {
        return true;
    }
    // tx.select(...) / trx.select(...)
    if (obj_text == "tx" || obj_text == "trx") && DB_METHODS.contains(&prop) {
        return true;
    }
    false
}

use oxc_span::GetSpan;

fn has_loop_ancestor<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    _source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_) => return true,
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(mem) = &call.callee {
                    let prop = mem.property.name.as_str();
                    if ARRAY_LOOP_METHODS.contains(&prop) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
    false
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
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
