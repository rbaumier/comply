use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ImportDeclarationSpecifier;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        // Already a top-level type import — fine.
        if import.import_kind.is_type() {
            return;
        }

        let Some(specifiers) = &import.specifiers else {
            return;
        };

        let mut total_named = 0usize;
        let mut type_count = 0usize;

        for spec in specifiers {
            let ImportDeclarationSpecifier::ImportSpecifier(s) = spec else {
                continue;
            };
            total_named += 1;
            if s.import_kind.is_type() {
                type_count += 1;
            }
        }

        if type_count == 0 {
            return;
        }

        let message = if type_count == total_named {
            "Prefer using a top-level `import type` instead of inline `type` specifiers."
        } else {
            "Split mixed imports: use a separate `import type` for type specifiers and a regular `import` for value specifiers."
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
