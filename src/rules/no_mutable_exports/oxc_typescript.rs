//! no-mutable-exports oxc backend — flag `export let` / `export var`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Declaration, VariableDeclarationKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
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
        let AstKind::ExportNamedDeclaration(export) = node.kind() else { return };
        let Some(decl) = &export.declaration else { return };
        let kind = match decl {
            Declaration::VariableDeclaration(var_decl) => match var_decl.kind {
                VariableDeclarationKind::Let => "let",
                VariableDeclarationKind::Var => "var",
                _ => return,
            },
            _ => return,
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Exporting mutable `{}` binding \u{2014} use `export const` instead.",
                kind
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
