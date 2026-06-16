//! enforce-update-with-where OxcCheck backend — flag `db.update(table)`
//! chains that have no `.where(...)` call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn receiver_looks_like_db(expr: &Expression, source: &str) -> bool {
    let name = leftmost_identifier(expr, source);
    let Some(name) = name else { return false };
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        "db" | "database" | "tx" | "trx" | "conn" | "client" | "drizzle" | "transaction"
    ) || lower.contains("db")
        || lower.contains("database")
        || name.ends_with("Tx")
        || name.ends_with("Db")
}

fn leftmost_identifier<'a>(expr: &'a Expression<'a>, source: &str) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str().to_owned()),
        Expression::StaticMemberExpression(member) => {
            Some(member.property.name.as_str().to_owned())
        }
        Expression::ComputedMemberExpression(member) => leftmost_identifier(&member.object, source),
        _ => None,
    }
}

/// Walk outward through `.method()` chain ancestors collecting method names.
fn collect_chain_methods<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    _source: &str,
) -> (oxc_span::Span, Vec<String>) {
    let AstKind::CallExpression(call) = node.kind() else {
        return (oxc_span::Span::new(0, 0), Vec::new());
    };
    let mut methods = Vec::new();
    let mut outer_span = call.span;

    // Walk ancestors: parent should be StaticMemberExpression, grandparent CallExpression.
    let mut current_id = node.id();
    loop {
        let mut ancestors = semantic.nodes().ancestors(current_id);
        let Some(parent) = ancestors.next() else {
            break;
        };
        let AstKind::StaticMemberExpression(member) = parent.kind() else {
            break;
        };
        if member.object.span().start != outer_span.start
            || member.object.span().end != outer_span.end
        {
            break;
        }
        let Some(grand) = ancestors.next() else {
            break;
        };
        let AstKind::CallExpression(grand_call) = grand.kind() else {
            break;
        };
        if grand_call.callee.span().start != member.span.start
            || grand_call.callee.span().end != member.span.end
        {
            break;
        }
        methods.push(member.property.name.as_str().to_string());
        outer_span = grand_call.span;
        current_id = grand.id();
    }

    (outer_span, methods)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".update("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "update" {
            return;
        }
        if !receiver_looks_like_db(&member.object, ctx.source) {
            return;
        }

        let (outer_span, methods) = collect_chain_methods(node, semantic, ctx.source);

        if methods.iter().any(|m| m == "where") {
            return;
        }

        // A `.where(...)` applied imperatively to a stored `let query = …`
        // binding (`query = query.where(filters)`) guards the query even though
        // it is absent from the static chain.
        if crate::oxc_helpers::where_applied_via_variable_reassignment(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, outer_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`db.update(...)` without `.where(...)` updates every row in the table — add a \
                      `.where(condition)` clause to bound the update."
                .into(),
            severity: Severity::Error,
            span: Some((outer_span.start as usize, outer_span.size() as usize)),
        });
    }
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
    fn flags_update_set_without_where() {
        let src = r#"const r = await db.update(users).set({ active: false });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_update_set_with_where() {
        let src = r#"const r = await db.update(users).set({ active: false }).where(eq(users.id, 1));"#;
        assert!(run(src).is_empty());
    }

    // Receivers absorbed from the removed drizzle-no-update-without-where rule.
    #[test]
    fn flags_transaction_receiver_without_where() {
        let src = r#"const r = await transaction.update(users).set({ active: false });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_tx_suffix_receiver_without_where() {
        let src = r#"const r = await userTx.update(users).set({ active: false });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_where_applied_via_conditional_reassignment() {
        let src = r#"
            function run(where, input, columns) {
                let query = db.update(users).set(input);
                if (where) {
                    const filters = extractFilters(users, "users", where);
                    query = query.where(filters) as any;
                }
                query = query.returning(columns) as any;
                const result = await query;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_stored_variable_reassigned_without_where() {
        let src = r#"
            function run(input, columns) {
                let query = db.update(users).set(input);
                query = query.returning(columns) as any;
                const result = await query;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_where_reassignment_of_other_variable() {
        let src = r#"
            function run(where, input, columns) {
                let query = db.update(users).set(input);
                let other = db.select();
                if (where) {
                    other = other.where(filters) as any;
                }
                query = query.returning(columns) as any;
                const result = await query;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
