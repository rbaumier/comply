use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".delete("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `obj.delete(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "delete" {
            return;
        }

        if !receiver_looks_like_db(&member.object, ctx.source) {
            return;
        }

        // Walk up the chain collecting method names.
        let methods = collect_chain_methods(node, semantic, ctx.source);

        if methods.iter().any(|m| m == "where") {
            return;
        }

        // A `.where(...)` applied imperatively to a stored `let query = …`
        // binding (`query = query.where(filters)`) guards the query even though
        // it is absent from the static chain.
        if crate::oxc_helpers::where_applied_via_variable_reassignment(node, semantic) {
            return;
        }

        // Report on the outermost call in the chain.
        let outer_span = outermost_call_span(node, semantic, ctx.source);
        let (line, column) = byte_offset_to_line_col(ctx.source, outer_span as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`db.delete(...)` without `.where(...)` removes every row in the table \u{2014} add a \
                      `.where(condition)` clause to bound the deletion."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Decide whether the receiver of `.delete(..)` looks like a database client.
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
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(member) => {
            // Prefer the property — `this.db` should resolve to `db`.
            Some(member.property.name.to_string())
        }
        Expression::ComputedMemberExpression(member) => {
            leftmost_identifier(&member.object, source)
        }
        Expression::ThisExpression(_) => Some("this".into()),
        _ => None,
    }
}

/// Starting from the `.delete(...)` call node, walk up through parent
/// chain-call nodes and collect their method names.
fn collect_chain_methods(
    start: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    _source: &str,
) -> Vec<String> {
    let mut methods = Vec::new();
    let nodes = semantic.nodes();
    let mut current_id = start.id();

    loop {
        let parent = nodes.parent_node(current_id);
        if parent.id() == current_id {
            break;
        }
        // Parent should be a StaticMemberExpression whose object is our current call.
        match parent.kind() {
            AstKind::StaticMemberExpression(member) => {
                let method_name = member.property.name.as_str();
                // Now the grandparent should be a CallExpression.
                let grand = nodes.parent_node(parent.id());
                if grand.id() == parent.id() {
                    break;
                }
                if matches!(grand.kind(), AstKind::CallExpression(_)) {
                    methods.push(method_name.to_string());
                    current_id = grand.id();
                    continue;
                }
                break;
            }
            _ => break,
        }
    }
    methods
}

/// Walk up the chain to find the outermost call's span start.
fn outermost_call_span(
    start: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    _source: &str,
) -> u32 {
    let nodes = semantic.nodes();
    let mut current_id = start.id();
    let mut outer_start = match start.kind() {
        AstKind::CallExpression(c) => c.span.start,
        _ => return 0,
    };

    loop {
        let parent = nodes.parent_node(current_id);
        if parent.id() == current_id {
            break;
        }
        match parent.kind() {
            AstKind::StaticMemberExpression(_) => {
                let grand = nodes.parent_node(parent.id());
                if grand.id() == parent.id() {
                    break;
                }
                if let AstKind::CallExpression(c) = grand.kind() {
                    outer_start = c.span.start;
                    current_id = grand.id();
                    continue;
                }
                break;
            }
            _ => break,
        }
    }
    outer_start
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
    fn flags_delete_without_where() {
        let src = r#"const r = await db.delete(users);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_delete_with_where() {
        let src = r#"const r = await db.delete(users).where(eq(users.id, 1));"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_set_delete() {
        let src = r#"const set = new Set(); set.delete(x);"#;
        assert!(run(src).is_empty());
    }

    // Receivers absorbed from the removed drizzle-no-delete-without-where rule.
    #[test]
    fn flags_transaction_receiver_without_where() {
        let src = r#"const r = await transaction.delete(users);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_tx_suffix_receiver_without_where() {
        let src = r#"const r = await userTx.delete(users);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_where_applied_via_conditional_reassignment() {
        let src = r#"
            function run(where, columns) {
                let query = db.delete(users);
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
            function run(columns) {
                let query = db.delete(users);
                query = query.returning(columns) as any;
                const result = await query;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_where_reassignment_of_other_variable() {
        let src = r#"
            function run(where, columns) {
                let query = db.delete(users);
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
