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
        "db" | "database" | "tx" | "trx" | "conn" | "client" | "drizzle"
    ) || lower.contains("db")
        || lower.contains("database")
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
        // Look for parent being a member expression where we are the object.
        let Some(parent) = semantic.nodes().ancestors(current_id).nth(1) else {
            break;
        };
        let AstKind::StaticMemberExpression(member) = parent.kind() else {
            break;
        };
        // Check that we are the object of this member expression.
        if member.object.span().start != outer_span.start
            || member.object.span().end != outer_span.end
        {
            break;
        }
        let Some(grand) = semantic.nodes().ancestors(parent.id()).nth(1) else {
            break;
        };
        let AstKind::CallExpression(grand_call) = grand.kind() else {
            break;
        };
        // Check that the member is the callee.
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
