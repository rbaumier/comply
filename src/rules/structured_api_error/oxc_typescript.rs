//! structured-api-error oxc backend — flag `new Error()` in route handler files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];

fn is_route_file(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim();
        ROUTE_METHODS.iter().any(|m| {
            let pat = format!(".{m}(");
            t.contains(&pat)
        }) || t.contains("from 'hono'")
            || t.contains("from \"hono\"")
            || t.contains("@hono/")
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "Error" {
            return;
        }

        if !is_route_file(ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bare `new Error()` in route handler \u{2014} use a structured error with `{ type, code, status, detail }`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
