//! hono-missing-secure-headers OXC backend — Hono app without secureHeaders().

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

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Only check Hono files.
        if !ctx.source_contains("from 'hono'") && !ctx.source_contains("from \"hono\"") {
            return Vec::new();
        }
        // Skip if secureHeaders is already imported.
        if ctx.source_contains("hono/secure-headers") {
            return Vec::new();
        }

        let mut has_routes = false;
        let mut hono_line: Option<usize> = None;

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::NewExpression(new_expr) => {
                    if let Expression::Identifier(id) = &new_expr.callee
                        && id.name.as_str() == "Hono" && hono_line.is_none() {
                            let (line, _) =
                                byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                            hono_line = Some(line);
                        }
                }
                AstKind::CallExpression(call) => {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        let name = member.property.name.as_str();
                        if matches!(name, "get" | "post" | "put" | "delete" | "patch") {
                            has_routes = true;
                        }
                    }
                }
                _ => {}
            }
        }

        if !has_routes {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: hono_line.unwrap_or(1),
            column: 1,
            rule_id: super::META.id.into(),
            message: "Hono app defines routes without `secureHeaders()` middleware \u{2014} security headers are missing.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}
