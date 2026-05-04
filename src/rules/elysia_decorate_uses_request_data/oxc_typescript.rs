//! elysia-decorate-uses-request-data oxc backend — flag `.decorate(...)` calls
//! whose argument list mentions `Date.now()` or `Math.random()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "decorate" {
            return;
        }
        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if !args_text.contains("Date.now()") && !args_text.contains("Math.random()") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.decorate(...)` runs once at boot \u{2014} `Date.now()` / `Math.random()` here freezes a single value for every request. Use `.derive(...)` for per-request data.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
