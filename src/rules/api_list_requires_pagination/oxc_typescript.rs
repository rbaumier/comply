//! api-list-requires-pagination oxc backend — flag exported `GET` handlers
//! when no pagination primitive is referenced anywhere in the file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, Statement, VariableDeclarationKind};
use std::sync::Arc;

const PAGINATION_TERMS: &[&str] = &["limit", "cursor", "page", "offset", "pageSize", "per_page"];

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
        if PAGINATION_TERMS.iter().any(|p| ctx.source.contains(p)) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for stmt in &semantic.nodes().program().body {
            let span = match stmt {
                Statement::ExportNamedDeclaration(export) => {
                    let Some(ref decl) = export.declaration else { continue };
                    match decl {
                        Declaration::FunctionDeclaration(f) => {
                            let Some(ref id) = f.id else { continue };
                            if id.name.as_str() != "GET" { continue; }
                            f.span
                        }
                        Declaration::VariableDeclaration(v) => {
                            if !matches!(v.kind, VariableDeclarationKind::Const | VariableDeclarationKind::Let) {
                                continue;
                            }
                            let mut found = None;
                            for decl in &v.declarations {
                                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = decl.id {
                                    if id.name.as_str() == "GET" {
                                        found = Some(v.span);
                                        break;
                                    }
                                }
                            }
                            let Some(s) = found else { continue };
                            s
                        }
                        _ => continue,
                    }
                }
                Statement::ExportDefaultDeclaration(export) => {
                    match &export.declaration {
                        oxc_ast::ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                            let Some(ref id) = f.id else { continue };
                            if id.name.as_str() != "GET" { continue; }
                            f.span
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "GET handler has no pagination — add `limit`/`cursor` or `page`/`pageSize` to prevent unbounded queries.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
