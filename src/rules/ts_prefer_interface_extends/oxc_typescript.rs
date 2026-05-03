//! ts-prefer-interface-extends oxc backend — flag `type X = A & B`
//! where every intersection member is a named type reference.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

pub struct Check;

fn is_named_type_ref(ty: &TSType) -> bool {
    matches!(ty, TSType::TSTypeReference(_))
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

        let TSType::TSIntersectionType(intersection) = &alias.type_annotation else { return };

        if intersection.types.len() < 2 {
            return;
        }
        if !intersection.types.iter().all(is_named_type_ref) {
            return;
        }

        let name = alias.id.name.as_str();
        let (line, column) =
            byte_offset_to_line_col(ctx.source, alias.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `interface {name} extends ...` over `type {name} = A & B` for object composition."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
