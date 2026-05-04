//! redundant-type-aliases oxc backend — flag `type X = Y` where Y is a single type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

pub struct Check;

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

        // Only flag if the alias has no type parameters (not generic).
        if alias.type_parameters.is_some() {
            return;
        }

        // Only flag if the RHS is a single type identifier or predefined type
        // (plain name like `Foo` or primitive like `string`).
        let is_simple = matches!(
            &alias.type_annotation,
            TSType::TSTypeReference(ref_ty)
                if ref_ty.type_arguments.is_none()
                    && matches!(
                        &ref_ty.type_name,
                        oxc_ast::ast::TSTypeName::IdentifierReference(_)
                    )
        ) || matches!(
            &alias.type_annotation,
            TSType::TSStringKeyword(_)
                | TSType::TSNumberKeyword(_)
                | TSType::TSBooleanKeyword(_)
                | TSType::TSAnyKeyword(_)
                | TSType::TSNeverKeyword(_)
                | TSType::TSNullKeyword(_)
                | TSType::TSUndefinedKeyword(_)
                | TSType::TSVoidKeyword(_)
                | TSType::TSBigIntKeyword(_)
                | TSType::TSSymbolKeyword(_)
                | TSType::TSObjectKeyword(_)
                | TSType::TSUnknownKeyword(_)
        );

        if !is_simple {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, alias.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Type alias is just renaming \u{2014} use the original type directly or add structure.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
