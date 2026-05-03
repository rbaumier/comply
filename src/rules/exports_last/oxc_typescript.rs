//! exports-last OXC backend — flag re-export statements that precede
//! non-export top-level statements.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

fn is_reexport(stmt: &Statement) -> bool {
    match stmt {
        Statement::ExportNamedDeclaration(decl) => {
            // A re-export has a source: `export { x } from './foo'`
            // or an export clause without a declaration: `export { x }`
            decl.source.is_some()
                || (decl.declaration.is_none() && !decl.specifiers.is_empty())
        }
        Statement::ExportAllDeclaration(_) => true,
        _ => false,
    }
}

fn is_export(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::ExportNamedDeclaration(_)
            | Statement::ExportDefaultDeclaration(_)
            | Statement::ExportAllDeclaration(_)
    )
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let body = &semantic.nodes().program().body;

        let has_reexports = body.iter().any(|s| is_reexport(s));
        if !has_reexports {
            return Vec::new();
        }

        // Find the index of the last non-export statement.
        let last_non_export_idx = body
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| !is_export(s))
            .map(|(i, _)| i);

        let Some(last_non_export_idx) = last_non_export_idx else {
            return Vec::new();
        };

        let mut diagnostics = Vec::new();
        for (i, stmt) in body.iter().enumerate() {
            if i >= last_non_export_idx {
                break;
            }
            if !is_reexport(stmt) {
                continue;
            }
            let span = match stmt {
                Statement::ExportNamedDeclaration(s) => s.span,
                Statement::ExportAllDeclaration(s) => s.span,
                _ => continue,
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Re-export statement is not at the end of the file.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
