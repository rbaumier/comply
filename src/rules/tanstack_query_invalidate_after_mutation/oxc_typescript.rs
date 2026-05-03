//! tanstack-query-invalidate-after-mutation OxcCheck backend.
//!
//! Detects `useMutation({ mutationFn: … })` whose `mutationFn` performs a
//! write (POST/PUT/PATCH/DELETE via fetch) but whose options object does not
//! include an `onSuccess` or `onSettled` callback containing
//! `invalidateQueries` / `setQueryData`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, ObjectExpression, ObjectPropertyKind, PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["mutationFn"])
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

        // Callee must be `useMutation`
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "useMutation" {
            return;
        }

        // First argument must be an object
        let Some(Argument::ObjectExpression(options)) = call.arguments.first() else {
            return;
        };

        // Must have `mutationFn` property
        let Some(mutation_fn) = find_property_value(options, "mutationFn") else {
            return;
        };
        if !is_write_mutation(mutation_fn, ctx.source) {
            return;
        }

        let has_handler = ["onSuccess", "onSettled"].iter().any(|name| {
            find_property_value(options, name)
                .is_some_and(|v| handler_calls_cache_update(v, ctx.source))
        });
        if has_handler {
            return;
        }

        let span = call.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useMutation` performs a write but does not update the query cache. \
                     Add `onSuccess` or `onSettled` that calls `invalidateQueries` or `setQueryData`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn find_property_value<'a>(
    obj: &'a ObjectExpression<'a>,
    needle: &str,
) -> Option<&'a Expression<'a>> {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        match &p.key {
            PropertyKey::StaticIdentifier(id) if id.name.as_str() == needle => {
                return Some(&p.value);
            }
            PropertyKey::StringLiteral(s) if s.value.as_str() == needle => {
                return Some(&p.value);
            }
            _ => {}
        }
    }
    None
}

/// True when the expression is a function whose body calls `fetch` with
/// a method of POST/PUT/PATCH/DELETE.
fn is_write_mutation(expr: &Expression<'_>, source: &str) -> bool {
    let body_source = match expr {
        Expression::ArrowFunctionExpression(arrow) => {
            &source[arrow.body.span.start as usize..arrow.body.span.end as usize]
        }
        Expression::FunctionExpression(func) => {
            let Some(body) = &func.body else {
                return false;
            };
            &source[body.span.start as usize..body.span.end as usize]
        }
        _ => return false,
    };

    // Quick text check for fetch + write method
    if !body_source.contains("fetch") {
        return false;
    }
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        // Check for method in quotes (single, double, backtick)
        let patterns = [
            format!("'{method}'"),
            format!("\"{method}\""),
            format!("`{method}`"),
        ];
        for pat in &patterns {
            if body_source.contains(pat.as_str()) {
                return true;
            }
        }
    }
    false
}

/// Check if a handler function body calls invalidateQueries/setQueryData/etc.
fn handler_calls_cache_update(expr: &Expression<'_>, source: &str) -> bool {
    let body_source = match expr {
        Expression::ArrowFunctionExpression(arrow) => {
            &source[arrow.body.span.start as usize..arrow.body.span.end as usize]
        }
        Expression::FunctionExpression(func) => {
            let Some(body) = &func.body else {
                return false;
            };
            &source[body.span.start as usize..body.span.end as usize]
        }
        _ => return false,
    };
    body_source.contains("invalidateQueries")
        || body_source.contains("setQueryData")
        || body_source.contains("refetchQueries")
        || body_source.contains("removeQueries")
}
