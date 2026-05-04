//! OXC backend for react-layout-requires-children-prop.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ExportDefaultDeclarationKind;
use std::sync::Arc;

pub struct Check;

fn is_layout_file(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem == "layout")
}

fn params_text_contains_children(source: &str, params_span: oxc_span::Span) -> bool {
    let text = &source[params_span.start as usize..params_span.end as usize];
    text.contains("children")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportDefaultDeclaration]
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
        if !ctx.file.path_segments.in_app_router {
            return;
        }
        if !is_layout_file(ctx.path) {
            return;
        }

        let AstKind::ExportDefaultDeclaration(export) = node.kind() else {
            return;
        };

        let params_span = match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => func.params.span,
            _ => return,
        };

        if params_text_contains_children(ctx.source, params_span) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "react-layout-requires-children-prop".into(),
            message: "This layout's default export doesn't accept `children`. \
                      The router passes nested routes via `children` — drop it and \
                      the layout renders an empty page."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
