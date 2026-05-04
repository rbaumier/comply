//! ts-no-unnecessary-type-constraint oxc backend — flag `<T extends any>` or
//! `<T extends unknown>`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::TSTypeParameter(param) = node.kind() else { continue };
            let Some(constraint) = &param.constraint else { continue };
            let keyword = match constraint {
                TSType::TSAnyKeyword(_) => "any",
                TSType::TSUnknownKeyword(_) => "unknown",
                _ => continue,
            };
            let (line, column) =
                byte_offset_to_line_col(ctx.source, constraint.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Unnecessary `extends {keyword}` constraint — \
                     all types already extend `{keyword}`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
