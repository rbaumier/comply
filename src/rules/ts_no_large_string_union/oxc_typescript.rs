//! OxcCheck backend for ts-no-large-string-union — flag unions with >N literal members.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

pub struct Check;

fn count_literals(ty: &TSType) -> usize {
    match ty {
        TSType::TSUnionType(union) => union.types.iter().map(count_literals).sum(),
        TSType::TSLiteralType(_) => 1,
        _ => 0,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else { return };

        let TSType::TSUnionType(union) = &alias.type_annotation else { return };

        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);
        let count: usize = union.types.iter().map(count_literals).sum();

        if count > max {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, union.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "String-literal union has {count} members (>{max}); consider a branded string or enum."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
