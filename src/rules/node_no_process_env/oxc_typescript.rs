//! OxcCheck backend — flag `process.env` usage.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["process"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::StaticMemberExpression(member) = node.kind() else { continue };
            if member.property.name.as_str() != "env" {
                continue;
            }
            let oxc_ast::ast::Expression::Identifier(obj) = &member.object else { continue };
            if obj.name.as_str() != "process" {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, member.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message:
                    "Unexpected use of `process.env`. Centralize environment access in a config module."
                        .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
