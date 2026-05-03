//! elysia-route-missing-params-schema OXC backend — flag routes with `:param`
//! placeholders but no `params:` schema in options.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

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

        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop_text = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&prop_text) {
            return;
        }

        // First argument should be a string literal path.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let arg_expr = first_arg.to_expression();
        let Expression::StringLiteral(path_lit) = arg_expr else {
            return;
        };
        let path = path_lit.value.as_str();

        // Check for `:param` segments.
        let has_param = path.split('/').any(|seg| seg.starts_with(':'));
        if !has_param {
            return;
        }

        // Check if `params:` appears in the full args text.
        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("params:") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route path declares `:param` but options have no `params:` schema — path params are unvalidated.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
