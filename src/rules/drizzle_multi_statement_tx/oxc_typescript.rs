//! drizzle-multi-statement-tx OXC backend.
//!
//! Within a function body, flag when 2+ `db.insert`/`db.update`/`db.delete`
//! calls appear and the enclosing function is not a `db.transaction(...)` callback.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const DB_RECEIVERS: &[&str] = &["db", "tx", "drizzle", "orm", "conn", "connection", "client"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["transaction"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let body_stmts: &[Statement] = match node.kind() {
            AstKind::Function(func) => {
                let Some(body) = &func.body else { return };
                &body.statements
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if arrow.expression { return; }
                &arrow.body.statements
            }
            _ => return,
        };

        // Skip if we're inside a transaction callback
        if is_in_transaction_callback(node, semantic, ctx.source) {
            return;
        }

        let mut mutation_spans: Vec<u32> = Vec::new();
        collect_block_mutations(body_stmts, ctx.source, &mut mutation_spans);

        if mutation_spans.len() < 2 {
            return;
        }

        let first_span = mutation_spans[0];
        let (line, column) = byte_offset_to_line_col(ctx.source, first_span as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Sequential `db.insert`/`db.update`/`db.delete` calls in the same scope — wrap them in `db.transaction(async (tx) => { ... })` so partial failures roll back.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_in_transaction_callback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::CallExpression(call) = parent.kind() {
            let callee_text = &source[call.callee.span().start as usize..call.callee.span().end as usize];
            if callee_text.ends_with(".transaction") || callee_text == "transaction" {
                return true;
            }
        }
        current_id = parent_id;
    }
}

fn is_db_mutation_call(call: &CallExpression, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let prop = member.property.name.as_str();
    if !matches!(prop, "insert" | "update" | "delete") {
        return false;
    }
    let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
    DB_RECEIVERS.iter().any(|r| obj_text == *r)
}

/// Walk down a chained call expression to find any inner `db.insert/update/delete`.
fn chain_contains_mutation(expr: &Expression, source: &str) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    if is_db_mutation_call(call, source) {
        return true;
    }
    // Check for chaining: the callee is a member expression whose object is another call
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    chain_contains_mutation(&member.object, source)
}

fn strip_await<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    if let Expression::AwaitExpression(a) = expr {
        &a.argument
    } else {
        expr
    }
}

fn collect_block_mutations(stmts: &[Statement], source: &str, out: &mut Vec<u32>) {
    for stmt in stmts {
        match stmt {
            Statement::ExpressionStatement(es) => {
                let inner = strip_await(&es.expression);
                if chain_contains_mutation(inner, source) {
                    out.push(inner.span().start);
                }
            }
            Statement::IfStatement(if_stmt) => {
                collect_stmt_mutations(&if_stmt.consequent, source, out);
                if let Some(alt) = &if_stmt.alternate {
                    collect_stmt_mutations(alt, source, out);
                }
            }
            Statement::TryStatement(try_stmt) => {
                collect_block_mutations(&try_stmt.block.body, source, out);
                if let Some(handler) = &try_stmt.handler {
                    collect_block_mutations(&handler.body.body, source, out);
                }
                if let Some(finalizer) = &try_stmt.finalizer {
                    collect_block_mutations(&finalizer.body, source, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_stmt_mutations(stmt: &Statement, source: &str, out: &mut Vec<u32>) {
    match stmt {
        Statement::BlockStatement(block) => {
            collect_block_mutations(&block.body, source, out);
        }
        Statement::ExpressionStatement(es) => {
            let inner = strip_await(&es.expression);
            if chain_contains_mutation(inner, source) {
                out.push(inner.span().start);
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_stmt_mutations(&if_stmt.consequent, source, out);
            if let Some(alt) = &if_stmt.alternate {
                collect_stmt_mutations(alt, source, out);
            }
        }
        _ => {}
    }
}
