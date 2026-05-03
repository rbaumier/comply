//! api-first oxc backend — flag files that register an HTTP route without
//! referencing any schema validator.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "delete"];
const SCHEMA_INDICATORS: &[&str] = &["z", "createRoute", "openapi", "schema", "zodValidator"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Quick check: if any schema indicator appears in source, skip.
        if SCHEMA_INDICATORS.iter().any(|s| ctx.source.contains(s)) {
            return Vec::new();
        }

        // Find the first route call: `<recv>.<method>(...)` with method in ROUTE_METHODS.
        let mut route_span = None;
        for snode in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = snode.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let method = member.property.name.as_str();
            if !ROUTE_METHODS.contains(&method) {
                continue;
            }
            let start = call.span.start;
            if route_span.map_or(true, |s: u32| start < s) {
                route_span = Some(start);
            }
        }

        let Some(span_start) = route_span else {
            return Vec::new();
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route handler without schema definition — define the API schema (e.g. `z.object`, `zodValidator`) before the handler.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}
