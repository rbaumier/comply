use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];
const DB_PATTERNS: &[&str] = &["prisma", "db", "knex"];
const DB_METHODS: &[&str] = &["findMany", "findFirst", "findUnique", "query"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if !is_db_call(call, ctx.source) {
            return;
        }

        if !is_in_route_handler(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-raw-db-entity-in-handler".into(),
            message: "Direct DB call in route handler — map to a DTO before returning.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_db_call(call: &oxc_ast::ast::CallExpression<'_>, source: &str) -> bool {
    // Direct calls like `knex("items")`
    if let Expression::Identifier(id) = &call.callee
        && DB_PATTERNS.contains(&id.name.as_str()) {
            return true;
        }
    // Member calls: check full text for DB pattern + method combination
    let text = &source[call.span.start as usize..call.span.end as usize];
    for pat in DB_PATTERNS {
        for method in DB_METHODS {
            if text.contains(pat) && text.contains(method) {
                return true;
            }
        }
    }
    false
}

fn is_in_route_handler(
    node: &oxc_semantic::AstNode<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let mut id = node.id();
    loop {
        let parent_id = nodes.parent_id(id);
        if parent_id == id {
            break;
        }
        id = parent_id;
        if let AstKind::CallExpression(call) = nodes.kind(id)
            && let Expression::StaticMemberExpression(member) = &call.callee {
                let method = member.property.name.as_str();
                if ROUTE_METHODS.contains(&method) {
                    return true;
                }
            }
    }
    false
}
