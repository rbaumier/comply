//! tanstack-start-server-fn-requires-validation OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const VALIDATION_METHODS: &[&str] = &["input", "safeParse", "parse"];

pub struct Check;

/// Walk a method-chained call expression to find the innermost `createServerFn()` span.
fn find_create_server_fn_span(expr: &oxc_ast::ast::Expression) -> Option<oxc_span::Span> {
    if let oxc_ast::ast::Expression::CallExpression(call) = expr {
        if let oxc_ast::ast::Expression::Identifier(id) = &call.callee {
            if id.name.as_str() == "createServerFn" {
                return Some(call.span);
            }
        }
        if let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee {
            return find_create_server_fn_span(&member.object);
        }
    }
    None
}

/// Return true when the first argument of the call is a function with no formal parameters.
fn handler_callback_has_no_params(call: &oxc_ast::ast::CallExpression) -> bool {
    match call.arguments.first() {
        Some(oxc_ast::ast::Argument::ArrowFunctionExpression(arrow)) => {
            arrow.params.items.is_empty()
        }
        Some(oxc_ast::ast::Argument::FunctionExpression(func)) => {
            func.params.items.is_empty()
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut server_fn_spans = Vec::new();
        let mut no_input_spans: Vec<oxc_span::Span> = Vec::new();
        let mut has_validation = false;

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };

            // Check for createServerFn(...)
            if let oxc_ast::ast::Expression::Identifier(id) = &call.callee
                && id.name.as_str() == "createServerFn"
            {
                server_fn_spans.push(call.span);
                continue;
            }

            if let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee {
                let method = member.property.name.as_str();

                // A .handler() whose callback has no parameters means no caller input.
                if method == "handler" && handler_callback_has_no_params(call) {
                    if let Some(span) = find_create_server_fn_span(&member.object) {
                        no_input_spans.push(span);
                    }
                }

                // Check for .input() / .safeParse() / .parse()
                if VALIDATION_METHODS.contains(&method) {
                    has_validation = true;
                }
            }
        }

        let server_fn_spans_needing_validation: Vec<_> = server_fn_spans
            .into_iter()
            .filter(|span| !no_input_spans.contains(span))
            .collect();

        if server_fn_spans_needing_validation.is_empty() || has_validation {
            return Vec::new();
        }

        server_fn_spans_needing_validation
            .into_iter()
            .map(|span| {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, span.start as usize);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`createServerFn` without `.input()` validation accepts unvalidated data at the RPC boundary.".into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}
