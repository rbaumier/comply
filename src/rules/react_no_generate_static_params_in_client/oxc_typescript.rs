//! react-no-generate-static-params-in-client oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["generateStaticParams"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.project.framework != Framework::NextJs {
            return;
        }
        if ctx.file.rsc_context != RscContext::ClientComponent {
            return;
        }

        let AstKind::ExportNamedDeclaration(export) = node.kind() else {
            return;
        };
        let Some(decl) = &export.declaration else {
            return;
        };
        let oxc_ast::ast::Declaration::FunctionDeclaration(f) = decl else {
            return;
        };
        let Some(id) = &f.id else {
            return;
        };
        if id.name.as_str() != "generateStaticParams" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`generateStaticParams` only runs in server components. \
                      Move it out of this `\"use client\"` file or the build \
                      silently skips pre-rendering."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
