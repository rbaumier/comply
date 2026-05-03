//! no-mass-assignment OXC backend — flag `{ ...req.body }` spread inside a DB write call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const DB_METHODS: &[&str] = &["set", "values", "insert", "update", "create"];

const USER_SPREAD_NEEDLES: &[&str] = &["...req.body", "...request.body"];

fn call_ends_with_db_method(callee_text: &str) -> bool {
    let tail = callee_text.rsplit('.').next().unwrap_or(callee_text);
    // Strip trailing `(` if present in slice
    let tail = tail.trim_end_matches('(');
    DB_METHODS.contains(&tail)
}

fn object_spreads_user_input(expr: &Expression, source: &str) -> bool {
    let Expression::ObjectExpression(obj) = expr else {
        return false;
    };
    for prop in &obj.properties {
        let ObjectPropertyKind::SpreadProperty(spread) = prop else {
            continue;
        };
        let text = &source[spread.span.start as usize..spread.span.end as usize];
        let trimmed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if USER_SPREAD_NEEDLES.iter().any(|n| trimmed.contains(n)) {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["...req.body", "...request.body"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if !call_ends_with_db_method(callee_text) {
            return;
        }

        for arg in &call.arguments {
            let arg_expr = match arg {
                Argument::SpreadElement(_) => continue,
                _ => arg.to_expression(),
            };
            if object_spreads_user_input(arg_expr, ctx.source) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Spreading `req.body` directly into a DB call allows mass-assignment — pick only the fields you need.".into(),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
        }
    }
}
