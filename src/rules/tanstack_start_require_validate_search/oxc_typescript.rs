//! tanstack-start-require-validate-search OXC backend — flag
//! `Route.useSearch()` in files that lack a `validateSearch:` option.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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
        // Bail early if source already contains `validateSearch`
        if ctx.source.contains("validateSearch") {
            return Vec::new();
        }

        // Find the first `Route.useSearch()` call
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let Expression::Identifier(obj) = &member.object else {
                continue;
            };
            if obj.name.as_str() != "Route" {
                continue;
            }
            if member.property.name.as_str() != "useSearch" {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            return vec![Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`Route.useSearch()` without `validateSearch:` in the route config accepts untyped search params.".into(),
                severity: Severity::Warning,
                span: None,
            }];
        }

        Vec::new()
    }
}
