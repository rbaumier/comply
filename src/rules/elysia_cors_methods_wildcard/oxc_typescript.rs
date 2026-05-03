//! elysia-cors-methods-wildcard OXC backend — flag credentialed `cors()` without
//! an explicit `methods` list.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        if ident.name != "cors" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(arg_expr) = first_arg.as_expression() else {
            return;
        };

        let args_text =
            &ctx.source[arg_expr.span().start as usize..arg_expr.span().end as usize];
        let norm: String = args_text.chars().filter(|c: &char| !c.is_whitespace()).collect();

        if !norm.contains("credentials:true") {
            return;
        }
        if norm.contains("methods:") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`credentials: true` without an explicit `methods` list — every HTTP verb is allowed.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
