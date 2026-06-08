//! OxcCheck backend for angular-no-barrel-file-side-effects — barrel `index.ts` should only re-export.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

fn is_barrel_path(path: &std::path::Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("index.ts") | Some("public-api.ts") | Some("public_api.ts")
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_barrel_path(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let program = semantic.source_text();
        let _ = program;
        for stmt in &semantic.nodes().program().body {
            let is_ok = matches!(
                stmt,
                Statement::ExportAllDeclaration(_)
                    | Statement::ExportNamedDeclaration(_)
                    | Statement::ExportDefaultDeclaration(_)
                    | Statement::ImportDeclaration(_)
                    | Statement::EmptyStatement(_)
            );
            if is_ok {
                continue;
            }
            let span = match stmt {
                Statement::ExpressionStatement(s) => s.span,
                Statement::BlockStatement(s) => s.span,
                Statement::VariableDeclaration(s) => s.span,
                Statement::FunctionDeclaration(s) => s.span,
                Statement::ClassDeclaration(s) => s.span,
                Statement::IfStatement(s) => s.span,
                Statement::ForStatement(s) => s.span,
                Statement::WhileStatement(s) => s.span,
                Statement::ReturnStatement(s) => s.span,
                Statement::ThrowStatement(s) => s.span,
                Statement::TryStatement(s) => s.span,
                Statement::SwitchStatement(s) => s.span,
                _ => oxc_span::Span::new(0, 0),
            };
            let snippet: String = ctx.source[span.start as usize..span.end as usize]
                .chars()
                .take(60)
                .collect();
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Barrel file should only re-export — found side-effecting statement: `{snippet}`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
