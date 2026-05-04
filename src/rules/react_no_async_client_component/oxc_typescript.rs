//! react-no-async-client-component OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use std::sync::Arc;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

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
        if ctx.file.rsc_context != RscContext::ClientComponent {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ExportDefaultDeclaration(export) => {
                    let oxc_ast::ast::ExportDefaultDeclarationKind::FunctionDeclaration(f) =
                        &export.declaration
                    else {
                        continue;
                    };
                    if !f.r#async {
                        continue;
                    }
                    let Some(name) = f.id.as_ref().map(|id| id.name.as_str()) else {
                        continue;
                    };
                    if !starts_with_uppercase(name) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, f.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "react-no-async-client-component".into(),
                        message: format!(
                            "`{name}` is an async client component. React client components \
                             must be synchronous — remove `async`, or drop `\"use client\"` \
                             to make this a server component."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                AstKind::ExportNamedDeclaration(export) => {
                    let Some(decl) = &export.declaration else {
                        continue;
                    };
                    let oxc_ast::ast::Declaration::FunctionDeclaration(f) = decl else {
                        continue;
                    };
                    if !f.r#async {
                        continue;
                    }
                    let Some(name) = f.id.as_ref().map(|id| id.name.as_str()) else {
                        continue;
                    };
                    if !starts_with_uppercase(name) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, f.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "react-no-async-client-component".into(),
                        message: format!(
                            "`{name}` is an async client component. React client components \
                             must be synchronous — remove `async`, or drop `\"use client\"` \
                             to make this a server component."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
                _ => {}
            }
        }

        diagnostics
    }
}
